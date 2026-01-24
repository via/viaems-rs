#![no_main]
#![no_std]

use panic_rtt_target as _;
use rtt_target::{rprintln, rtt_init_print};

use stm32f4::stm32f427 as pac;

use cortex_m_rt::entry;
use spin_on::spin_on;
use futures_lite::future::zip;


#[entry]
fn entry() -> ! {

    let firstloop = async {
        loop {}
    };

    let secondloop = async {
        loop { await 1; }
    };

    spin_on(zip(firstloop, secondloop));
    loop {
    }

}


