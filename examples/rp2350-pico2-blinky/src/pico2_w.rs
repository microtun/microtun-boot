//! Pico 2 W on-board LED.
//!
//! The LED is not wired to the RP2350: it hangs off WL_GPIO0 of the CYW43439
//! radio. The radio talks gSPI over a single PIO state machine (GPIO23 = WL_ON,
//! GPIO24 = data, GPIO25 = chip select, GPIO29 = clock), and only answers once
//! its firmware has been uploaded, so the LED needs the full radio bring-up.

use cyw43::aligned_bytes;
use cyw43_pio::{PioSpi, RM2_CLOCK_DIVIDER};
use embassy_executor::Spawner;
use embassy_rp::{
    Peri, bind_interrupts, dma,
    gpio::{Level, Output},
    peripherals::{DMA_CH0, PIN_23, PIN_24, PIN_25, PIN_29, PIO0},
    pio::{self, Pio},
};
use log::info;
use static_cell::StaticCell;

use crate::{StatusLed, blink_forever};

bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => pio::InterruptHandler<PIO0>;
    DMA_IRQ_0 => dma::InterruptHandler<DMA_CH0>;
});

/// Everything the radio needs, handed over from `main`.
pub struct Radio {
    pub pio: Peri<'static, PIO0>,
    pub dma: Peri<'static, DMA_CH0>,
    /// WL_ON: radio power/reset.
    pub pwr: Peri<'static, PIN_23>,
    /// gSPI data in/out (shared, half duplex).
    pub dio: Peri<'static, PIN_24>,
    /// gSPI chip select.
    pub cs: Peri<'static, PIN_25>,
    /// gSPI clock.
    pub clk: Peri<'static, PIN_29>,
}

type Cyw43Runner = cyw43::Runner<'static, cyw43::SpiBus<Output<'static>, PioSpi<'static, PIO0, 0>>>;

impl StatusLed for cyw43::Control<'static> {
    async fn set(&mut self, on: bool) {
        self.gpio_set(0, on).await; // WL_GPIO0, active high
    }
}

/// Services the radio's bus; the LED commands go through it.
#[embassy_executor::task]
async fn cyw43_task(runner: Cyw43Runner) -> ! {
    runner.run().await
}

/// Brings the radio up, then blinks the on-board LED.
#[embassy_executor::task]
pub async fn blink(spawner: Spawner, radio: Radio) -> ! {
    // Infineon firmware from embassy-rs/embassy `cyw43-firmware/`, see
    // `cyw43-firmware/` in this example for the licence.
    let fw = aligned_bytes!("../cyw43-firmware/43439A0.bin");
    let clm = aligned_bytes!("../cyw43-firmware/43439A0_clm.bin");
    let nvram = aligned_bytes!("../cyw43-firmware/nvram_rp2040.bin");

    let pwr = Output::new(radio.pwr, Level::Low);
    let cs = Output::new(radio.cs, Level::High);
    let mut pio = Pio::new(radio.pio, Irqs);
    let spi = PioSpi::new(
        &mut pio.common,
        pio.sm0,
        // The Pico 2 W's radio module does not cope with the default 75 MHz
        // PIO clock; RM2_CLOCK_DIVIDER gives 25 MHz gSPI at 150 MHz.
        RM2_CLOCK_DIVIDER,
        pio.irq0,
        cs,
        radio.dio,
        radio.clk,
        dma::Channel::new(radio.dma, Irqs),
    );

    static STATE: StaticCell<cyw43::State> = StaticCell::new();
    let state = STATE.init(cyw43::State::new());
    let (_net_device, mut control, runner) = cyw43::new(state, pwr, spi, fw, nvram).await;
    spawner.spawn(cyw43_task(runner).expect("spawn cyw43"));

    control.init(clm).await;
    control
        .set_power_management(cyw43::PowerManagementMode::PowerSave)
        .await;
    info!("cyw43 up, on-board LED on WL_GPIO0");

    blink_forever(&mut control).await
}
