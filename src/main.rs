#![no_std]
#![no_main]

// Demo of using non-blocking DMA transactions with the
// TWIS (Two Wire Interface/I2C in peripheral mode) module.

use {core::panic::PanicInfo, nrf52840_hal as hal, rtt_target::rprintln};

#[rtic::app(device = crate::hal::pac, peripherals = true, dispatchers = [SWI0_EGU0])]
mod app {

    use {
        hal::{
            gpio::{p0::Parts, p1::Parts as Parts1},
            gpiote::Gpiote,
            pac::{TWIM1, TWIS0},
            twim::{Pins as TwimPins, *},
            twis::{Pins as TwisPins, *},
        },
        nrf52840_hal as hal,
        rtt_target::{rprintln, rtt_init_print},
    };

    type DmaBuffer = &'static mut [u8; 8];

    pub enum TwisTransfer {
        Running(Transfer<TWIS0, DmaBuffer>),
        Idle((DmaBuffer, Twis<TWIS0>)),
    }

    #[shared]
    struct Shared {
        #[lock_free]
        transfer: Option<TwisTransfer>,
    }

    #[local]
    struct Local {
        gpiote: Gpiote,
        twim: Twim<TWIM1>,
    }

    #[init(local = [
        BUF: [u8; 8] = [0; 8],
    ])]
    fn init(ctx: init::Context) -> (Shared, Local, init::Monotonics) {
        let BUF = ctx.local.BUF;

        let _clocks = hal::clocks::Clocks::new(ctx.device.CLOCK).enable_ext_hfosc();
        rtt_init_print!();
        rprintln!("Waiting for commands from controller...");

        let p0 = Parts::new(ctx.device.P0);
        let p1 = Parts1::new(ctx.device.P1); // nrf52840_mdk has its button connected to p1_00

        // Configure gpio pins 15 and 16 for TWIS
        let scl = p0.p0_15.into_floating_input().degrade();
        let sda = p0.p0_16.into_floating_input().degrade();

        // create a twis instance
        let twis = Twis::new(ctx.device.TWIS0, TwisPins { scl, sda }, 0x1A);
        twis.enable_interrupt(TwiEvent::Write)
            .enable_interrupt(TwiEvent::Read)
            .enable_interrupt(TwiEvent::Stopped)
            .enable();

        // Configure gpio pins 26 and 27 for TWIM
        let scl = p0.p0_27.into_floating_input().degrade();
        let sda = p0.p0_26.into_floating_input().degrade();

        // create a twim instance
        let twim = Twim::new(ctx.device.TWIM1, TwimPins { scl, sda }, Frequency::K100);

        // button to reset DMA buffer
        let btn = p1.p1_00.into_pullup_input().degrade();

        // gpio tasks and events instance
        let gpiote = Gpiote::new(ctx.device.GPIOTE);
        gpiote.port().input_pin(&btn).low();
        gpiote.port().enable_interrupt();

        (
            Shared {
                transfer: Some(TwisTransfer::Idle((BUF, twis))),
            },
            Local { gpiote, twim },
            init::Monotonics(),
        )
    }

    #[task(priority = 2, binds = GPIOTE, local = [gpiote], shared = [transfer])]
    fn on_gpiote(ctx: on_gpiote::Context) {
        ctx.local.gpiote.reset_events();
        rprintln!("Reset buffer");
        let transfer = ctx.shared.transfer;
        let (buf, twis) = match transfer.take().unwrap() {
            TwisTransfer::Running(t) => t.wait(),
            TwisTransfer::Idle(t) => t,
        };
        buf.copy_from_slice(&[0; 8][..]);
        rprintln!("{:?}", buf);
        transfer.replace(TwisTransfer::Idle((buf, twis)));

        // spawn `send_twi_cmds` task. This task uses the `twim` to send read and write commands to `twis`.
        send_twi_cmds::spawn().unwrap();
    }

    #[task(priority = 2, binds = SPIM0_SPIS0_TWIM0_TWIS0_SPI0_TWI0, shared = [transfer])]
    fn on_twis(ctx: on_twis::Context) {
        let transfer = ctx.shared.transfer;
        let (buf, twis) = match transfer.take().unwrap() {
            TwisTransfer::Running(t) => t.wait(),
            TwisTransfer::Idle(t) => t,
        };
        if twis.is_event_triggered(TwiEvent::Read) {
            twis.reset_event(TwiEvent::Read);
            rprintln!("READ command received");
            let tx = twis.tx(buf).unwrap();
            transfer.replace(TwisTransfer::Running(tx));
        } else if twis.is_event_triggered(TwiEvent::Write) {
            twis.reset_event(TwiEvent::Write);
            rprintln!("WRITE command received");
            let rx = twis.rx(buf).unwrap();
            transfer.replace(TwisTransfer::Running(rx));
        } else {
            twis.reset_event(TwiEvent::Stopped);
            rprintln!("{:?}", buf);
            transfer.replace(TwisTransfer::Idle((buf, twis)));
        }
    }

    #[task(local = [twim])]
    fn send_twi_cmds(ctx: send_twi_cmds::Context) {
        let twim = ctx.local.twim;

        // read 8 bytes from TWIS at address 0x1A
        rprintln!("\nREAD from address 0x1A");
        let rx_buf = &mut [0; 8][..];
        let res = twim.read(0x1A, rx_buf);
        rprintln!("Result: {:?}\n{:?}", res, rx_buf);

        // write 8 bytes to TWIS at address 0x1A
        rprintln!("\nWRITE to address 0x1A");
        let tx_buf = [1, 2, 3, 4, 5, 6, 7, 8];
        let res = twim.write(0x1A, &tx_buf[..]);
        rprintln!("Result: {:?}\n{:?}", res, tx_buf);
    }

    #[idle]
    fn idle(_cx: idle::Context) -> ! {
        rprintln!("idle");

        loop {
            // Now Wait For Interrupt is used instead of a busy-wait loop
            // to allow MCU to sleep between interrupts
            // https://developer.arm.com/documentation/ddi0406/c/Application-Level-Architecture/Instruction-Details/Alphabetical-list-of-instructions/WFI
            rtic::export::wfi()
        }
    }
}

#[inline(never)]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    cortex_m::interrupt::disable();
    rprintln!("{}", info);
    loop {}
}
