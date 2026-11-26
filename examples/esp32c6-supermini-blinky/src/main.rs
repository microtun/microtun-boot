#![no_std]
#![no_main]
//! ESP32-C6 SuperMini + microtun boot + Embassy example.
//!
//! * Blinks the on-board addressable RGB LED (a WS2812-type LED on GPIO8,
//!   driven by the RMT peripheral). The colour and rhythm show the boot
//!   state: fast **amber** flashes while a trial (test) image is still
//!   unconfirmed, a slow **green** heartbeat once the image is permanent.
//! * Pressing the BOOT button (GPIO9) prints the microtun boot handoff info.
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
    Async,
    clock::CpuClock,
    gpio::{Input, InputConfig, Level, Pull},
    rmt::{Channel, PulseCode, Rmt, Tx, TxChannelConfig, TxChannelCreator},
    rtc_cntl::Rwdt,
    time::Rate,
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

/// Colours, kept dim: the LED is very bright at full scale.
const AMBER: Rgb = Rgb(24, 10, 0);
const GREEN: Rgb = Rgb(0, 20, 0);
const OFF: Rgb = Rgb(0, 0, 0);

#[derive(Clone, Copy)]
struct Rgb(u8, u8, u8);

/// Minimal driver for the board's single WS2812-type LED on GPIO8.
/// This is not the separate active-high status LED on GPIO15.
/// The RGB LED is switched off by sending zero for all colour channels.
///
/// The RMT runs at 80 MHz (12.5 ns per tick). Each colour bit is one RMT
/// pulse code: a `1` is 0.8 µs high then 0.45 µs low, a `0` is 0.4 µs high
/// then 0.85 µs low. The LED latches the 24 bits (GRB order, MSB first) once
/// the line has stayed low for more than 50 µs, which the idle level and the
/// pause between updates guarantee.
struct Ws2812 {
    channel: Channel<'static, Async, Tx>,
}

impl Ws2812 {
    const RMT_MHZ: u32 = 80;
    const ONE: PulseCode = PulseCode::new(Level::High, 64, Level::Low, 36);
    const ZERO: PulseCode = PulseCode::new(Level::High, 32, Level::Low, 68);

    async fn write(&mut self, Rgb(r, g, b): Rgb) {
        let mut codes = [PulseCode::end_marker(); 25];
        for (i, byte) in [g, r, b].into_iter().enumerate() {
            for bit in 0..8 {
                codes[i * 8 + bit] = if byte & (0x80 >> bit) != 0 {
                    Self::ONE
                } else {
                    Self::ZERO
                };
            }
        }
        if let Err(e) = self.channel.transmit(&codes).await {
            warn!("LED update failed: {:?}", e);
        }
    }
}

#[embassy_executor::task]
async fn watchdog(mut wdt: Rwdt) -> ! {
    loop {
        wdt.feed();
        Timer::after_secs(1).await;
    }
}

#[embassy_executor::task]
async fn blink(mut led: Ws2812) -> ! {
    loop {
        let (colour, period) = if CONFIRMED.load(Ordering::Relaxed) {
            (GREEN, Duration::from_millis(1_000))
        } else {
            (AMBER, Duration::from_millis(150))
        };
        led.write(colour).await;
        Timer::after(Duration::from_millis(50)).await;
        led.write(OFF).await;
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

    let rmt = Rmt::new(p.RMT, Rate::from_mhz(Ws2812::RMT_MHZ))
        .expect("RMT clock")
        .into_async();
    let channel = rmt
        .channel0
        .configure_tx(
            &TxChannelConfig::default()
                .with_clk_divider(1)
                .with_idle_output_level(Level::Low)
                .with_idle_output(true),
        )
        .expect("RMT TX channel")
        .with_pin(p.GPIO8);
    let boot_button = Input::new(p.GPIO9, InputConfig::default().with_pull(Pull::Up));
    spawner.spawn(blink(Ws2812 { channel }).expect("spawn blink"));
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
