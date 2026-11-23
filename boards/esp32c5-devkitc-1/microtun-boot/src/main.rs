#![no_std]
#![no_main]
#![deny(unsafe_op_in_unsafe_fn)]
//! microtun boot third-stage boot manager for the ESP32-C5.
//!
//! Boot chain: ROM -> ESP-IDF second stage (0x2000) -> signed factory microtun boot
//! (0x20000) -> the application image in `image-0` (or `image-1` in
//! direct-XIP mode), loaded by a RAM handoff loader.

use core::cell::RefCell;

use log::{error, info, warn};
use microtun_boot_core::{Error, PublicKey, flash::ProgressFlash};
use microtun_boot_esp::{boot_runtime, chip, glitch, loader};

mod config;

// Standard IDF app descriptor; with linkall.x, this should land at binary 0x20.
esp_bootloader_esp_idf::esp_app_desc!();

include!(concat!(env!("OUT_DIR"), "/keys.rs"));

/// The flash as the boot decision sees it: every completed operation feeds
/// the watchdog.
type BootFlash<'d, 'w> = ProgressFlash<microtun_boot_esp::FlashStorage<'d>, &'w dyn Fn()>;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo<'_>) -> ! {
    esp_println::println!("microtun-boot panic: {}", info);
    boot_runtime::reset_after_delay()
}

#[esp_hal::main]
fn main() -> ! {
    // ESP-IDF installs the C5 platform map before HAL startup. The shared
    // boot runtime also invalidates stale boot info and clears ROM flash-boot
    // protection before long-running slot validation or swaps.
    let peripherals = boot_runtime::init();
    // Arm the watchdog before any long-running step. It is fed after every
    // completed flash operation and runs on, never stopped, into the handoff.
    let watchdog = RefCell::new(boot_runtime::start_watchdog(peripherals.RTC_TIMER));
    let feed_watchdog = || watchdog.borrow_mut().feed();
    esp_println::logger::init_logger(log::LevelFilter::Info);
    info!(
        "microtun boot {} for {}, mode {:?}",
        env!("CARGO_PKG_VERSION"),
        chip::NAME,
        config::MODE
    );
    // Re-assert the glitch detector reset ESP-IDF's second stage should have
    // enabled, before slot validation, swaps or eFuse access.
    match glitch::enable() {
        Ok(report) => {
            if report.reset_by_glitch {
                warn!("last reset was caused by a glitch detector");
            }
            info!("{} detector reset enabled", glitch::DETECTOR);
        }
        Err(e) => {
            error!("glitch detector setup failed: {:?}", e);
            boot_runtime::reset_after_delay()
        }
    }

    // Randomness for the delays in front of fault-hardened checks; its
    // entropy source is released before the handoff.
    let mut rng = boot_runtime::BootRng::new(peripherals.RNG, peripherals.ADC1);

    let storage = microtun_boot_esp::open_flash(peripherals.FLASH);
    let flash_size = storage.lock(|storage| storage.borrow().capacity()) as u32;
    info!("flash size: {} KiB", flash_size / 1024);
    let layout = microtun_boot_esp::read_layout(&storage)
        .expect("failed to read slot layout from partition table");
    // From here on, all flash access feeds the watchdog.
    let flash = microtun_boot_esp::share(BootFlash::new(
        storage.into_inner().into_inner(),
        &feed_watchdog,
    ));

    info!(
        "primary {:#x}+{:#x}, secondary {:#x}+{:#x}, scratch {:?}",
        layout.primary.offset,
        layout.primary.size,
        layout.secondary.offset,
        layout.secondary.size,
        layout.scratch
    );

    let keys = PUBLIC_KEYS.map(|k| PublicKey::from_raw(&k).expect("invalid built-in public key"));

    // Lets direct-XIP honour `imgtool --rom-fixed` like upstream.
    let mut boot_config = config::boot_config();
    boot_config.slot_addresses = Some([layout.primary.offset, layout.secondary.offset]);

    // Boot-time fuse access is exclusive; no peripheral IRQs are needed here.
    // The IDF second stage leaves APP_ANTI_ROLLBACK disabled; microtun boot owns
    // SECURE_VERSION independently of the factory boot manager's app descriptor.
    riscv::interrupt::disable();
    // SAFETY: `esp_hal::init` ran in `boot_runtime::init`; single core with
    // IRQs disabled for the counter's lifetime, and nothing else in this
    // binary programs eFuses; SECURE_VERSION is reserved for microtun boot and
    // IDF anti-rollback is off (see above).
    let mut security_counter =
        unsafe { microtun_boot_esp::security_counter::EfuseSecurityCounter::new() };
    let (decision, max_image_size) = {
        let mut bootloader = microtun_boot_esp::bootloader(&layout, &flash, &keys, boot_config)
            .expect("invalid slot configuration")
            .with_security_counter_store(&mut security_counter)
            .with_entropy(&mut rng);

        let decision = match bootloader.boot_with_validator(
            &mut |partition: &mut microtun_boot_esp::StoragePartition<'_, BootFlash<'_, '_>>,
                  _: microtun_boot_core::Slot,
                  target: microtun_boot_core::Slot,
                  image: &microtun_boot_core::ValidatedImage| {
                let header_size = u32::from(image.header().hdr_size);
                let destination = layout
                    .region(target)
                    .offset
                    .checked_add(header_size)
                    .ok_or(Error::Image(
                        microtun_boot_core::ImageError::PlatformRejected,
                    ))?;
                match loader::validate(partition, header_size, destination, image.header().img_size)
                {
                    Ok(()) => Ok(()),
                    Err(loader::LoadError::Flash(error)) => Err(Error::Flash(error)),
                    Err(error) => {
                        error!("rejecting application executable: {:?}", error);
                        Err(Error::Image(
                            microtun_boot_core::ImageError::PlatformRejected,
                        ))
                    }
                }
            },
        ) {
            Ok(d) => d,
            Err(Error::Flash(e)) => {
                // Possibly transient; everything is restartable.
                error!("flash error: {:?}; resetting", e);
                boot_runtime::reset_after_delay()
            }
            Err(e) => {
                error!("unable to find a bootable image: {:?}", e);
                boot_runtime::reset_after_delay()
            }
        };
        (decision, bootloader.max_image_size())
    };

    let region = layout.region(decision.slot);
    let image = region.offset + u32::from(decision.image.header().hdr_size);
    info!(
        "starting image {} from {:?} slot (ESP image at {:#x})",
        decision.image.header().version,
        decision.slot,
        image
    );

    let rollback_floor = match microtun_boot_core::SecurityCounterStore::get(&mut security_counter)
    {
        Ok(floor) => floor,
        Err(e) => {
            error!("cannot read hardware rollback floor: {:?}; resetting", e);
            boot_runtime::reset_after_delay()
        }
    };
    let boot_info = match microtun_boot_esp::boot_info::boot_save_shared_data(
        config::MODE,
        decision.slot,
        max_image_size,
        Some(config::version()),
        Some(rollback_floor),
    ) {
        Ok(info) => info,
        Err(e) => {
            error!("cannot build shared boot information: {:?}; resetting", e);
            boot_runtime::reset_after_delay()
        }
    };

    let plan = flash.lock(|storage| {
        loader::plan(
            &mut *storage.borrow_mut(),
            image,
            decision.image.header().img_size,
            boot_info,
        )
    });
    let plan = match plan {
        Ok(plan) => plan,
        Err(e) => {
            error!("cannot load the application image: {:?}", e);
            boot_runtime::reset_after_delay();
        }
    };
    info!("jumping to {:#010x}", plan.entry());
    // Point of no return: nothing is printed after this. Authenticate the
    // slot a second time, re-derive the plan and re-read the rollback floor,
    // then decide after a random delay. A disagreement panics (resetting).
    let plan = loader::confirm(
        plan,
        &flash,
        loader::Authenticated {
            region,
            image: &decision.image,
            keys: &keys,
            policy: &boot_config.policy,
            max_image_size,
        },
        &mut security_counter,
        &mut rng,
    );
    rng.release();
    // Leave the hardware timer running through the destructive RAM handoff,
    // with a full deadline for the application's startup.
    feed_watchdog();
    loader::execute(plan)
}
