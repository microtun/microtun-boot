#![no_std]
#![no_main]
//! RP2350 Pico 2 + microtun boot + Embassy example.
//!
//! * Blinks the on-board green LED. The blink pattern shows the boot state:
//!   fast while a trial (test) image is still unconfirmed, slow once the image
//!   is permanent. On the Pico 2 the LED is on GPIO25 (active high). On the
//!   Pico 2 W (`pico2-w` feature) it is WL_GPIO0 of the CYW43439 radio, so the
//!   radio is started first and the LED is switched through it.
//! * Logs the microtun boot handoff info (mode, slot, image size, bootloader).
//! * After a short self-test the running image confirms itself, so a swap
//!   upgrade is kept instead of being reverted on the next reset.

use core::{
    cell::RefCell,
    sync::atomic::{AtomicBool, Ordering},
};

use critical_section::Mutex;
use embassy_executor::Spawner;
use embassy_rp::{
    gpio::{Level, Output},
    uart::UartTx,
    watchdog::Watchdog,
};
use embassy_time::{Duration, Timer};
use log::{error, info, warn};
use microtun_boot_core::flash::ProgressFlash;
use microtun_boot_rp::{BootInfo, hardware, uart_logger};

#[cfg(feature = "pico2-w")]
mod pico2_w;

/// Pico 2 flash map, checked at compile time against the RP2350's rules. It
/// must match the layout the stock microtun boot bootloader was built with.
const LAYOUT: microtun_boot_rp::Layout =
    microtun_boot_rp::Layout::new(microtun_boot_layout_rp2350_pico2::FLASH_MAP);

/// Same timeout as the bootloader, below the RP2350's 24-bit (16.7 s) limit.
const WATCHDOG_TIMEOUT: Duration = Duration::from_secs(15);

/// How long the application must run healthily before it confirms itself.
const SELF_TEST: Duration = Duration::from_secs(5);

/// Set once the running image is confirmed (or did not need confirming).
static CONFIRMED: AtomicBool = AtomicBool::new(false);

/// The watchdog microtun boot left running. Shared, because both the feeder task
/// and the flash driver (during long erase/program operations) feed it.
static WATCHDOG: Mutex<RefCell<Option<Watchdog>>> = Mutex::new(RefCell::new(None));

fn feed_watchdog() {
    critical_section::with(|cs| {
        if let Some(wdt) = WATCHDOG.borrow_ref_mut(cs).as_mut() {
            wdt.feed(WATCHDOG_TIMEOUT);
        }
    });
}

// Hand-written on purpose: no `no_std` panic crate logs through `log` and then
// resets (`panic-reset` drops the message, `panic-probe` needs defmt or RTT),
// and the message should reach the same UART console as microtun boot's.
#[panic_handler]
fn panic(info: &core::panic::PanicInfo<'_>) -> ! {
    error!("panic: {}", info);
    // An unconfirmed trial image that keeps resetting is reverted by microtun boot.
    cortex_m::peripheral::SCB::sys_reset()
}

// Reset through microtun boot straight away instead of waiting for the watchdog.
#[cortex_m_rt::exception]
unsafe fn HardFault(_frame: &cortex_m_rt::ExceptionFrame) -> ! {
    cortex_m::peripheral::SCB::sys_reset()
}

#[embassy_executor::task]
async fn watchdog() -> ! {
    loop {
        feed_watchdog();
        Timer::after_secs(1).await;
    }
}

/// The on-board status LED: a GPIO on the Pico 2, a radio GPIO on the Pico 2 W.
trait StatusLed {
    async fn set(&mut self, on: bool);
}

impl StatusLed for Output<'static> {
    async fn set(&mut self, on: bool) {
        self.set_level(Level::from(on)); // active high
    }
}

/// Blink pattern: fast while the image is an unconfirmed trial, slow after.
async fn blink_forever(led: &mut impl StatusLed) -> ! {
    loop {
        let period = if CONFIRMED.load(Ordering::Relaxed) {
            Duration::from_millis(1_000)
        } else {
            Duration::from_millis(150)
        };
        led.set(true).await;
        Timer::after(Duration::from_millis(50)).await;
        led.set(false).await;
        Timer::after(period).await;
    }
}

#[cfg(not(feature = "pico2-w"))]
#[embassy_executor::task]
async fn blink(mut led: Output<'static>) -> ! {
    blink_forever(&mut led).await
}

#[embassy_executor::main(
    executor = "embassy_rp::executor::Executor",
    entry = "cortex_m_rt::entry"
)]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    // Take over the watchdog microtun boot started; it keeps running.
    critical_section::with(|cs| {
        WATCHDOG
            .borrow_ref_mut(cs)
            .replace(Watchdog::new(p.WATCHDOG))
    });
    feed_watchdog();
    // Same console as the bootloader: UART0 TX on GPIO0, 115200 8N1.
    uart_logger::init(UartTx::new_blocking(
        p.UART0,
        p.PIN_0,
        uart_logger::config(),
    ));

    spawner.spawn(watchdog().expect("spawn watchdog"));

    // The handoff record microtun boot left in reserved RAM. Without it we do not
    // know which slot we run from, so refuse to touch the update machinery.
    let Some(boot) = BootInfo::read() else {
        error!("no microtun boot handoff found; was this image started by microtun boot?");
        loop {
            Timer::after_secs(60).await;
        }
    };

    #[cfg(not(feature = "pico2-w"))]
    spawner.spawn(blink(Output::new(p.PIN_25, Level::Low)).expect("spawn blink"));
    // The radio starts in its own task, so a radio fault cannot hold up the
    // self-test and confirmation below.
    #[cfg(feature = "pico2-w")]
    spawner.spawn(
        pico2_w::blink(
            spawner,
            pico2_w::Radio {
                pio: p.PIO0,
                dma: p.DMA_CH0,
                pwr: p.PIN_23,
                dio: p.PIN_24,
                cs: p.PIN_25,
                clk: p.PIN_29,
            },
        )
        .expect("spawn blink"),
    );

    // Slots come from the board layout crate, exactly as microtun boot finds them.
    // ProgressFlash feeds the watchdog between flash operations.
    let flash = microtun_boot_rp::share(ProgressFlash::new(
        hardware::flash::<{ LAYOUT.flash_size() }>(p.FLASH),
        feed_watchdog,
    ));
    let mut updater = microtun_boot_rp::updater(&LAYOUT, &boot, &flash)
        .expect("bootloader handoff and slot layout disagree");

    let active = updater.inspect_active().expect("running image header");
    info!(
        "hello from image {} in {:?} (mode {:?})",
        active.version,
        boot.active_slot(),
        boot.mode()
    );
    info!(
        "microtun-boot: max_image={:?} bootloader={:?}",
        boot.max_image_size(),
        boot.bootloader_version(),
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
