#![no_main]
#![no_std]

extern crate cortex_m;
#[macro_use]
extern crate cortex_m_rt as rt;
extern crate panic_semihosting;
extern crate stm32f103xx_hal as hal;
#[macro_use]
extern crate stm32f103xx as device;
extern crate pwm_speaker;

use rt::ExceptionFrame;
use hal::prelude::*;

entry!(main);

#[derive(Copy, Clone)]
enum ButtonManager {
    UpState(u8),
    DownState(u8),
}
impl ButtonManager {
    fn is_pressed(&mut self, value: bool) -> bool {
        use ButtonManager::*;
        match self {
            UpState(cnt) => if value { *cnt = 0 } else { *cnt += 1 },
            DownState(cnt) => if value { *cnt += 1 } else { *cnt = 0 },
        }
        match *self {
            UpState(cnt) if cnt >= 30 => {
                *self = DownState(0);
                return true;
            }
            DownState(cnt) if cnt >= 30 => *self = UpState(0),
            _ => (),
        }
        return false;
    }
}

static mut BUTTON: Option<hal::gpio::gpiob::PB0<hal::gpio::Input<hal::gpio::Floating>>> =
    None;
static mut LED: Option<hal::gpio::gpioc::PC13<hal::gpio::Output<hal::gpio::PushPull>>> =
    None;
static mut SPEAKER: Option<pwm_speaker::Speaker> = None;
static mut SONG: Option<core::iter::Cycle<pwm_speaker::songs::Events>> = None;
static mut WAIT_SONG: u32 = 0;

fn main() -> ! {
    let dp = hal::stm32f103xx::Peripherals::take().unwrap();
    let mut cp = cortex_m::Peripherals::take().unwrap();
    let mut flash = dp.FLASH.constrain();
    let mut rcc = dp.RCC.constrain();
    let mut afio = dp.AFIO.constrain(&mut rcc.apb2);
    let clocks = rcc.cfgr.freeze(&mut flash.acr);

    let mut gpioc = dp.GPIOC.split(&mut rcc.apb2);
    let led = gpioc.pc13.into_push_pull_output(&mut gpioc.crh);

    let mut gpiob = dp.GPIOB.split(&mut rcc.apb2);
    let button = gpiob.pb0.into_floating_input(&mut gpiob.crl);
    let mut timer = hal::timer::Timer::tim3(dp.TIM3, 1.khz(), clocks, &mut rcc.apb1);
    timer.listen(hal::timer::Event::Update);
    cp.NVIC.enable(hal::stm32f103xx::Interrupt::TIM3);

    let mut gpioa = dp.GPIOA.split(&mut rcc.apb2);
    let c1 = gpioa.pa0.into_alternate_push_pull(&mut gpioa.crl);
    let mut pwm = dp
        .TIM2
        .pwm(c1, &mut afio.mapr, 440.hz(), clocks, &mut rcc.apb1);
    pwm.enable();
    let speaker = pwm_speaker::Speaker::new(pwm, clocks);

    unsafe {
        BUTTON = Some(button);
        LED = Some(led);
        SPEAKER = Some(speaker);
        SONG = Some(pwm_speaker::songs::LAVENTURIER.events().cycle());
    }

    loop {
        cortex_m::asm::wfi();
    }
}

interrupt!(TIM3, tim3, state: ButtonManager = ButtonManager::UpState(0));
fn tim3(manager: &mut ButtonManager) {
    unsafe { (*device::TIM3::ptr()).sr.modify(|_, w| w.uif().clear_bit()); };
    let button = unsafe { BUTTON.as_ref().unwrap() };
    let led = unsafe { LED.as_mut().unwrap() };
    let speaker = unsafe { SPEAKER.as_mut().unwrap() };
    let wait_song = unsafe { &mut WAIT_SONG };

    if manager.is_pressed(button.is_high()) {
        if led.is_set_low() {
            led.set_high();
        } else {
            led.set_low();
            speaker.mute();
            *wait_song = 0;
        }
    }

    if led.is_set_low() { return; }
    if *wait_song > 0 {
        *wait_song -= 1;
        return;
    }
    use pwm_speaker::songs::Event::*;
    match unsafe { SONG.as_mut().unwrap().next().unwrap() } {
        Note { pitch, ms } => {
            speaker.play((pitch as u32).hz());
            *wait_song = ms;
        }
        Rest { ms } => {
            speaker.mute();
            *wait_song = ms;
        }
    };
}

exception!(HardFault, hard_fault);

fn hard_fault(ef: &ExceptionFrame) -> ! {
    panic!("{:#?}", ef);
}

exception!(*, default_handler);

fn default_handler(irqn: i16) {
    panic!("Unhandled exception (IRQn = {})", irqn);
}
