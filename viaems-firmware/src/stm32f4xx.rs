use fugit;
use rtic_time::{Monotonic, TimerQueue};
//use stm32f4::stm32f427::*;
use stm32_metapac as pac;

use rtic_monotonics::systick::Systick;

pub struct Stm32f427 {
}

pub struct Scheduler {
}

impl Scheduler {
    pub fn new() -> Self {
        Scheduler { }
    }
}

static TIMER_QUEUE : TimerQueue<Stm32f427> = TimerQueue::new();

impl Monotonic for Stm32f427 {

    type Instant = fugit::TimerInstantU32<4_000_000>;
    type Duration = fugit::TimerDurationU32<4_000_000>;

    const ZERO : Self::Instant = Self::Instant::from_ticks(0);
    const TICK_PERIOD : Self::Duration = Self::Duration::from_ticks(0);

    fn now() -> Self::Instant {
        let counter = pac::TIM2.cnt().read().0;
        Self::Instant::from_ticks(counter)
    }

    fn set_compare(instant: Self::Instant) {
        pac::TIM2.ccr(3).write(|w|  w.set_ccr(instant.ticks()));
    }
    fn clear_compare_flag() {
        pac::TIM2.sr().modify(|w| w.set_ccif(3, false));
    }

    fn pend_interrupt() {
        pac::TIM2.egr().write(|w| w.set_ccg(3, true));
    }

    fn on_interrupt() {
    }
}

unsafe impl Sync for Stm32f427 {}

impl Stm32f427 {
    // Configure TIM8 to run at HCLK speed and overflow at 4
    fn configure_tim8() {
        let tim8 = pac::TIM8;

        tim8.arr().write(|w| w.set_arr(41)); // 42 == 4 MHz
        tim8.dier().write(|w| w.set_uie(true));

        tim8.cr2().write(|w| w.set_mms(pac::timer::vals::Mms::UPDATE));
        tim8.cr1().write(|w| w.set_cen(true));
    }

    pub fn configure_tim2() {
        let tim2 = pac::TIM2;

        tim2.smcr().write(|w| {
            w.set_sms(pac::timer::vals::Sms::EXT_CLOCK_MODE);
            w.set_ts(pac::timer::vals::Ts::ITR1);
        });

        tim2.arr().write(|w| w.set_arr(0xffffffff));
        tim2.dier().write(|w| {
            w.set_ccie(0, true);
            w.set_ccie(1, true);
            w.set_ccie(3, true);
        });

        tim2.cr1().modify(|w| w.set_cen(true));
    }

    fn enable_peripheral_clocks() {
        let rcc = pac::RCC;
        rcc.apb1enr().modify(|w| {
            w.set_pwren(true);
            w.set_tim2en(true);
            w.set_tim3en(true);
        });

        rcc.apb2enr().modify(|w| {
            w.set_tim1en(true);
            w.set_tim8en(true);
            w.set_tim9en(true);
            w.set_spi1en(true);
        });

        rcc.ahb1enr().modify(|w| {
            w.set_gpioaen(true);
            w.set_gpioben(true);
            w.set_gpiocen(true);
            w.set_gpioden(true);
            w.set_gpioeen(true);
            w.set_dma1en(true);
            w.set_dma2en(true);
        });

        rcc.ahb2enr().modify(|w| w.set_usb_otg_fsen(true));
    }

    fn setup_clock(xtal_freq_mhz: u8) {
        let rcc = pac::RCC;

        // Turn on HSE
        rcc.cr().modify(|w| w.set_hseon(true));
        while !rcc.cr().read().hserdy() {}

        // Configure PLL
        rcc.pllcfgr().write(|w| {
            w.set_pllsrc(pac::rcc::vals::Pllsrc::HSE);
            w.set_pllm(pac::rcc::vals::Pllm::from_bits(xtal_freq_mhz));
            w.set_plln(pac::rcc::vals::Plln::from_bits(336));
            w.set_pllq(pac::rcc::vals::Pllq::from_bits(7));
            w.set_pllp(pac::rcc::vals::Pllp::from_bits(0));
        });

        // Turn on PLL
        rcc.cr().modify(|w| w.set_pllon(true));
        while !rcc.cr().read().pllrdy() {}

        // Turn on overdrive
        let pwr = pac::PWR;
        pwr.cr1().modify(|w| w.set_oden(true));
        while !pwr.csr1().read().odrdy() {}

        pwr.cr1().modify(|w| w.set_odswen(true));
        while !pwr.csr1().read().odswrdy() {}

        rcc.cfgr().write(|w| {
            w.set_ppre2(pac::rcc::vals::Ppre::DIV2);
            w.set_ppre1(pac::rcc::vals::Ppre::DIV4);
            w.set_hpre(pac::rcc::vals::Hpre::DIV1);
        });

        let flash = pac::FLASH;
        flash.acr().write(|w| {
            w.set_latency(pac::flash::vals::Latency::WS5);
            w.set_icen(true);
            w.set_dcen(true);
            w.set_prften(true);
        });


        rcc.cfgr().modify(|w| w.set_sw(pac::rcc::vals::Sw::PLL1_P));
        while rcc.cfgr().read().sws() != pac::rcc::vals::Sw::PLL1_P {}
    }

    fn setup_gpio() {
        let gpioe = pac::GPIOE;
        gpioe.odr().write(|w| w.set_odr(0, pac::gpio::vals::Odr::HIGH));
        gpioe.moder().write(|w| w.0 = 0x55555555);
        gpioe.ospeedr().write(|w| w.0 = 0xffffffff);
    }

    fn setup_itm() {
        use pac::gpio::vals;

        let gpioe = pac::GPIOE;
        gpioe.moder().modify(|w| w.set_moder(3, vals::Moder::ALTERNATE));
        gpioe.afr(0).modify(|w| w.set_afr(3, 0));

        let tpiu = cortex_m::peripheral::TPIU::PTR;
        unsafe { (*tpiu).acpr.write(1) };
    }

    pub fn toggle() {
        pac::GPIOE
            .odr()
            .modify(|w| 
                if w.odr(0) == pac::gpio::vals::Odr::LOW {
                    w.set_odr(0, pac::gpio::vals::Odr::HIGH);
                } else {
                    w.set_odr(0, pac::gpio::vals::Odr::LOW);
                }
            );
    }

    pub fn cycles() -> u32 {
      unsafe { (*cortex_m::peripheral::DWT::PTR).cyccnt.read() }
    }

    pub fn with_itm<F>(&mut self, f: F) 
    where F: FnOnce(&mut cortex_m::peripheral::itm::Stim) {
        let stim = unsafe { &mut ((*cortex_m::peripheral::ITM::PTR).stim[0]) };
        f(stim);
    }

    pub async fn delay(duration: <Self as Monotonic>::Duration) {
        TIMER_QUEUE.delay(duration).await;
    }

//    pub fn now() -> <Self as Monotonic>::Instant {
//        <Self as Monotonic>::now()
//    }

    pub fn init() {
        Stm32f427::setup_itm();

        Stm32f427::enable_peripheral_clocks();
        Stm32f427::setup_clock(8);
        Stm32f427::configure_tim8();
        Stm32f427::configure_tim2();
        Stm32f427::setup_gpio();

        TIMER_QUEUE.initialize(Self {});

    }
}
