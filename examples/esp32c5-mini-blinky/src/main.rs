#![no_std]
#![no_main]
//! ESP32-C5 Mini + microtun boot + Embassy example.
//!
//! * Blinks the on-board user LED (GPIO23, active low). The blink pattern
//!   shows the boot state: fast while a trial (test) image is still
//!   unconfirmed, slow once the image is permanent.
//! * Pressing the BOOT button (GPIO28) prints the microtun boot handoff info.
//! * After a short self-test the running image confirms itself, so a swap
//!   upgrade is kept instead of being reverted on the next reset.
//!
//! microtun boot hands off with a running 15 s RTC watchdog, but `esp_hal::init`
//! disables it, so the application re-arms it immediately and feeds it from
//! its own task.

use core::sync::atomic::{AtomicBool, Ordering};

use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock,
    gpio::{Input, InputConfig, Level, Output, OutputConfig, Pull},
    rtc_cntl::Rwdt,
    timer::timg::TimerGroup,
};
use log::{error, info, warn};
use microtun_boot_esp::BootInfo;

// ESP-IDF application descriptor. microtun boot's ESP image conversion expects it.
esp_bootloader_esp_idf::esp_app_desc!();

/// Set once the running image is confirmed (or did not need confirming).
static CONFIRMED: AtomicBool = AtomicBool::new(false);

/// How long the application must run healthily before it confirms itself.
const SELF_TEST: Duration = Duration::from_secs(5);

/// The NiceMCU ESP32-C5 Mini user LED on GPIO23 is active low.
const LED_ON: Level = Level::Low;

#[embassy_executor::task]
async fn watchdog(mut wdt: Rwdt) -> ! {
    loop {
        wdt.feed();
        Timer::after_secs(1).await;
    }
}

#[embassy_executor::task]
async fn blink(mut led: Output<'static>) -> ! {
    loop {
        let period = if CONFIRMED.load(Ordering::Relaxed) {
            Duration::from_millis(1_000)
        } else {
            Duration::from_millis(150)
        };
        led.set_level(LED_ON);
        Timer::after(Duration::from_millis(50)).await;
        led.set_level(!LED_ON);
        Timer::after(period).await;
    }
}

#[embassy_executor::task]
async fn button(mut boot_button: Input<'static>, boot: BootInfo) -> ! {
    loop {
        boot_button.wait_for_falling_edge().await;
        info!(
            "microtun-boot: mode={:?} slot={:?} max_image={:?} bootloader={:?} confirmed={}",
            boot.mode(),
            boot.active_slot(),
            boot.max_image_size(),
            boot.bootloader_version(),
            CONFIRMED.load(Ordering::Relaxed),
        );
        // Crude debounce.
        Timer::after_millis(200).await;
    }
}

#[esp_hal::main]
async fn main(spawner: Spawner) {
    let p = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));
    // Re-arm the watchdog that esp_hal::init just disabled.
    let wdt =
        microtun_boot_esp::watchdog::start_rtc(p.RTC_TIMER, esp_hal::time::Duration::from_secs(15));
    esp_println::logger::init_logger(log::LevelFilter::Info);

    let timg0 = TimerGroup::new(p.TIMG0);
    esp_rtos::start(timg0.timer0, p.FROM_CPU_INTR0);

    spawner.spawn(watchdog(wdt).expect("spawn watchdog"));

    // The handoff record microtun boot left in reserved RAM. Without it we do not
    // know which slot we run from, so refuse to touch the update machinery.
    let Some(boot) = BootInfo::read() else {
        error!("no microtun boot handoff found; was this image started by microtun boot?");
        loop {
            Timer::after_secs(60).await;
        }
    };

    let led = Output::new(p.GPIO23, !LED_ON, OutputConfig::default());
    let boot_button = Input::new(p.GPIO28, InputConfig::default().with_pull(Pull::Up));
    spawner.spawn(blink(led).expect("spawn blink"));
    spawner.spawn(button(boot_button, boot).expect("spawn button"));

    // Slots come from the partition table in flash, not from compile-time
    // constants, exactly as microtun boot finds them.
    let flash = microtun_boot_esp::open_flash(p.FLASH);
    let layout = microtun_boot_esp::read_layout(&flash).expect("partition table");
    let mut updater = microtun_boot_esp::updater(&layout, &boot, &flash)
        .expect("bootloader handoff and slot layout disagree");

    let active = updater.inspect_active().expect("running image header");
    info!(
        "hello from image {} in {:?} (mode {:?})",
        active.version,
        boot.active_slot(),
        boot.mode()
    );

    if updater.needs_confirmation().expect("read trailer") {
        warn!(
            "trial image: self-testing for {} s before confirming",
            SELF_TEST.as_secs()
        );
        Timer::after(SELF_TEST).await;
        // Put real health checks here (sensors answer, network reachable, ...).
        // If they fail, just reset: microtun boot reverts an unconfirmed image.
        updater.confirm().expect("confirm image");
        info!("image {} confirmed", active.version);
    } else {
        info!("image already permanent");
    }
    CONFIRMED.store(true, Ordering::Relaxed);
}
