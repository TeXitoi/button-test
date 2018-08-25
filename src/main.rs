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
extern crate embedded_hal;

use rt::ExceptionFrame;
use hal::prelude::*;

entry!(main);

#[derive(Copy, Clone)]
pub enum ButtonEvent {
    Pressed,
    Reseased,
    Nothing,
}
#[derive(Copy, Clone)]
enum ButtonState {
    HighState(u8),
    LowState(u8),
}
struct ButtonManager<T> {
    button: T,
    state: ButtonState,
}
impl<T: embedded_hal::digital::InputPin> ButtonManager<T> {
    pub fn new(button: T) -> Self {
        ButtonManager {
            button,
            state: ButtonState::HighState(0)
        }
    }
    pub fn poll(&mut self) -> ButtonEvent {
        use ButtonState::*;
        let value = self.button.is_high();
        match &mut self.state {
            HighState(cnt) => if value { *cnt = 0 } else { *cnt += 1 },
            LowState(cnt) => if value { *cnt += 1 } else { *cnt = 0 },
        }
        match self.state {
            HighState(cnt) if cnt >= 30 => {
                self.state = LowState(0);
                ButtonEvent::Pressed
            }
            LowState(cnt) if cnt >= 30 => {
                self.state = HighState(0);
                ButtonEvent::Reseased
            }
            _ => ButtonEvent::Nothing,
        }
    }
}

type ButtonPB0 = hal::gpio::gpiob::PB0<hal::gpio::Input<hal::gpio::Floating>>;
static mut BUTTON: Option<ButtonManager<ButtonPB0>> = None;

type LedPC13 = hal::gpio::gpioc::PC13<hal::gpio::Output<hal::gpio::PushPull>>;
static mut LED: Option<LedPC13> = None;

static mut SPEAKER: Option<pwm_speaker::Speaker> = None;
static mut SONG: Option<core::iter::Cycle<pwm_speaker::songs::MsEvents>> = None;

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
        BUTTON = Some(ButtonManager::new(button));
        LED = Some(led);
        SPEAKER = Some(speaker);
        SONG = Some(pwm_speaker::songs::THIRD_KIND.events().ms_events().cycle());
    }

    loop {
        cortex_m::asm::wfi();
    }
}

interrupt!(TIM3, tim3);
fn tim3() {
    unsafe { (*device::TIM3::ptr()).sr.modify(|_, w| w.uif().clear_bit()); };
    let button = unsafe { BUTTON.as_mut().unwrap() };
    let led = unsafe { LED.as_mut().unwrap() };
    let speaker = unsafe { SPEAKER.as_mut().unwrap() };

    if let ButtonEvent::Pressed = button.poll() {
        if led.is_set_low() {
            led.set_high();
            speaker.unmute();
        } else {
            led.set_low();
            speaker.mute();
        }
    }

    if led.is_set_low() { return; }
    use pwm_speaker::songs::MsEvent::*;
    match unsafe { SONG.as_mut().unwrap().next().unwrap() } {
        BeginNote { pitch } => speaker.play(pitch),
        EndNote => speaker.rest(),
        Wait => (),
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
