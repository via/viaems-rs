
use rtic_monotonics::systick::*;
use rtt_target::rprintln;
use crate::app;

pub async fn exttest(cx: app::exttest::Context<'_>) {
    loop { 
        Systick::delay(5000.millis()).await;
        panic!("Debug line from exttest");
    }
}
