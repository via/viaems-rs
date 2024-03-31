use fugit;
use rtic_time::Monotonic;
use stm32f4::stm32f427::*;

use rtic_monotonics::systick::Systick;

pub struct Stm32f427 {
    mono: Option<Scheduler>,
    gpioe: GPIOE,
    itm: ITM,
}

pub struct Scheduler {
    tim2: TIM2,
}

impl Scheduler {
    pub fn new(tim2: TIM2) -> Self {
        Scheduler { tim2 }
    }
}

impl Monotonic for Scheduler {
    type Instant = fugit::TimerInstantU32<4_000_000>;
    type Duration = fugit::TimerDurationU32<4_000_000>;

    const ZERO : Self::Instant = Self::Instant::from_ticks(0);
    const TICK_PERIOD : Self::Duration = Self::Duration::from_ticks(0);

    fn now() -> Self::Instant {
      //  let counter = self.tim2.cnt.read().bits();
        let counter = 0;
        Self::Instant::from_ticks(counter)
    }

    fn set_compare(instant: Self::Instant) {
//        self.tim2.ccr4().write(|w| w.bits(instant.ticks()));
    }
    fn clear_compare_flag() {
//        self.tim2.sr.modify(|_, w| w.cc4if().clear_bit());
    }

    fn pend_interrupt() {
    }

    fn on_interrupt() {
    }
}

unsafe impl Sync for Stm32f427 {}

impl Stm32f427 {
    // Configure TIM8 to run at HCLK speed and overflow at 4
    fn configure_tim8(tim8: &TIM8) {
        tim8.arr.write(|w| w.arr().bits(41)); // 42 == 4 MHz
        tim8.dier.write(|w| w.uie().enabled());

        tim8.cr2.write(|w| w.mms().update());
        tim8.cr1.write(|w| w.cen().enabled());
    }

    fn configure_tim2(tim2: &TIM2) {
        tim2.smcr.write(|w| {
            w.sms().ext_clock_mode();
            w.ts().itr1();
            w
        });

        tim2.arr.write(|w| w.arr().bits(0xffffffff));
        tim2.dier.write(|w| {
            w.cc1ie().set_bit();
            w.cc2ie().set_bit();
            w.cc4ie().set_bit();
            w
        });
        tim2.cr1.modify(|_r, w| w.cen().set_bit());
    }

    fn enable_peripheral_clocks(rcc: &RCC) {
        rcc.apb1enr.modify(|_, w| {
            w.pwren().enabled();
            w.tim2en().enabled();
            w.tim3en().enabled();
            w
        });

        rcc.apb2enr.modify(|_, w| {
            w.tim1en().enabled();
            w.tim8en().enabled();
            w.tim9en().enabled();
            w.spi1en().enabled();
            w
        });

        rcc.ahb1enr.modify(|_, w| {
            w.gpioaen().enabled();
            w.gpioben().enabled();
            w.gpiocen().enabled();
            w.gpioden().enabled();
            w.gpioeen().enabled();
            w.dma1en().enabled();
            w.dma2en().enabled();
            w
        });

        rcc.ahb2enr.modify(|_, w| w.otgfsen().enabled());
    }

    fn setup_clock(periphs: &Peripherals, xtal_freq_mhz: u8) {
        let rcc = &periphs.RCC;

        // Turn on HSE
        rcc.cr.modify(|_, w| w.hseon().on());
        while rcc.cr.read().hserdy().is_not_ready() {}

        // Configure PLL
        rcc.pllcfgr.write(|w| unsafe {
            w.pllsrc().hse();
            w.pllm().bits(xtal_freq_mhz);
            w.plln().bits(336);
            w.pllq().bits(7);
            w.pllp().bits(0);
            w
        });

        // Turn on PLL
        rcc.cr.modify(|_, w| w.pllon().on());
        while rcc.cr.read().pllrdy().is_not_ready() {}

        // Turn on overdrive
        let pwr = &periphs.PWR;
        pwr.cr.modify(|_, w| w.oden().set_bit());
        while pwr.csr.read().odrdy().bit_is_clear() {}

        pwr.cr.modify(|_, w| w.odswen().set_bit());
        while pwr.csr.read().odswrdy().bit_is_clear() {}

        rcc.cfgr.write(|w| {
            w.ppre2().div2();
            w.ppre1().div4();
            w.hpre().div1();
            w
        });

        let flash = &periphs.FLASH;
        flash.acr.write(|w| {
            w.latency().bits(5);
            w.icen().set_bit();
            w.dcen().set_bit();
            w.prften().set_bit();
            w
        });

        while rcc.cr.read().pllrdy().bit_is_clear() {}

        rcc.cfgr.modify(|_, w| w.sw().pll());
    }

    fn setup_gpio(gpioe: &GPIOE) {
        gpioe.odr.write(|w| w.odr1().set_bit());
        gpioe.moder.write(|w| unsafe { w.bits(0x55555555) });
        gpioe.ospeedr.write(|w| unsafe { w.bits(0xffffffff) });
    }

    fn setup_itm(core: &CorePeripherals, periphs: &Peripherals) {
        periphs.GPIOB.moder.modify(|_, w| w.moder3().alternate());
        periphs.GPIOB.afrl.modify(|_, w| w.afrl3().af0());

        unsafe { core.TPIU.acpr.write(1) };
    }

    pub fn monotonic(&mut self) -> Scheduler {
        self.mono.take().unwrap()
    }

    pub fn toggle(&self) {
        self.gpioe
            .odr
            .modify(|r, w| w.odr1().bit(r.odr1().bit_is_clear()));
    }

    pub fn cycles() -> u32 {
      unsafe { (*DWT::PTR).cyccnt.read() }
    }

    pub fn with_itm<F>(&mut self, f: F) 
    where F: FnOnce(&mut cortex_m::peripheral::itm::Stim) {
        f(&mut self.itm.stim[0]);
    }

    pub fn new(periphs: Peripherals, core: CorePeripherals) -> Self {
        Stm32f427::setup_itm(&core, &periphs);

        Stm32f427::enable_peripheral_clocks(&periphs.RCC);
        Stm32f427::setup_clock(&periphs, 8);
        Stm32f427::configure_tim8(&periphs.TIM8);
        Stm32f427::configure_tim2(&periphs.TIM2);
        Stm32f427::setup_gpio(&periphs.GPIOE);

        let mono = Scheduler::new(periphs.TIM2);
        let plat = Stm32f427 {
            mono: Some(mono),
            gpioe: periphs.GPIOE,
            itm: core.ITM,
        };

        let systick_mono_token = rtic_monotonics::create_systick_token!();
        Systick::start(core.SYST, 168_000_000, systick_mono_token);

        plat
    }
}
