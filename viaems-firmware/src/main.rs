#![no_main]
#![no_std]

use panic_rtt_target as _;
use rtic::app;
use rtic_monotonics::systick::*;
use rtic_sync::make_channel;
use rtic_sync::channel::{Sender, Receiver};
use rtt_target::{rprintln, rtt_init_print};

use stm32f4::stm32f427 as pac;
mod stm32f4xx;
use stm32f4xx::Stm32f427 as platform;

#[app(device = pac, peripherals = true, dispatchers = [SPI1, SPI2, SPI3])]
mod app {
    use super::*;

    #[shared]
    struct Shared {
        engine_position: EnginePosition,
        sensors: Sensors,
    }

    #[local]
    struct Local {
        state: bool,
        logger_queue_receiver: Receiver<'static, LogMsg, 16>, 
        logger_queue_decoder_sender: Sender<'static, LogMsg, 16>, 
        logger_queue_engine_sender: Sender<'static, LogMsg, 16>, 
    }

    #[derive(Debug)]
    enum LogMsg {
        DecodeMsg{
            orig_time: u32,
            send_time: u32,
        },
        EngineMsg{
            orig_time: u32,
            send_time: u32,
        },
    }

    #[derive(Default)]
    pub struct Trigger {
        time: u32,
        trigger: u32,
    }

    #[derive(Default)]
    pub struct RawAdc {
        time: u32,
        valid: bool,
        values: [u16; 16],
    }

    #[derive(Default)]
    pub struct EnginePosition {
        valid_since: u32,
        rpm: f32,
        angle: f32,
    }

    #[derive(Default)]
    pub struct Sensors {
        map: f32,
        iat: f32,
    }

    #[init]
    fn init(cx: init::Context) -> (Shared, Local) {


        rtt_init_print!();
        rprintln!("init");

        sim::spawn().unwrap();
        logger::spawn().unwrap();

        let (s, r) = make_channel!(LogMsg, 16);

        let mut platform = platform::new(cx.device, cx.core);

        (Shared {
           engine_position: EnginePosition::default(),
           sensors: Sensors::default(),
        }, Local { 
            state: false ,
            logger_queue_receiver: r,
            logger_queue_decoder_sender: s.clone(),
            logger_queue_engine_sender: s.clone(),
        })
    }


    #[task(local=[logger_queue_engine_sender], shared=[engine_position], priority=3)]
    async fn engine(mut cx: engine::Context) {
          #[allow(deprecated)]
          cx.shared.engine_position.lock(|p| {
              let cycles = cortex_m::peripheral::DWT::get_cycle_count();
              cx.local.logger_queue_engine_sender.try_send(LogMsg::EngineMsg{
                  orig_time: p.valid_since, 
                  send_time: cycles
              }).ok();
          });
    }

    #[task( 
        local=[logger_queue_decoder_sender], 
        shared=[engine_position], priority=3)]
    async fn decode(mut cx: decode::Context, trigger: Trigger) {
        cx.shared.engine_position.lock(|p| {
            p.valid_since = trigger.time;
        });

        #[allow(deprecated)]
        let cycles = cortex_m::peripheral::DWT::get_cycle_count();
        cx.local.logger_queue_decoder_sender.try_send(LogMsg::DecodeMsg{
            orig_time: trigger.time,
            send_time: cycles,
        }).ok();

        engine::spawn().ok();
    }

    #[task(local=[logger_queue_receiver], priority=1)]
    async fn logger(cx: logger::Context) {
        loop {
          match cx.local.logger_queue_receiver.recv().await {
              Ok(LogMsg::DecodeMsg{orig_time, send_time}) => {
                  #[allow(deprecated)]
                  let cycles = cortex_m::peripheral::DWT::get_cycle_count();
                  rprintln!("DecodeMsg: orig_time {} send_time {}",
                      (cycles - orig_time), (send_time - orig_time));

              },
              Ok(LogMsg::EngineMsg{orig_time, send_time}) => {
                  #[allow(deprecated)]
                  let cycles = cortex_m::peripheral::DWT::get_cycle_count();
                  rprintln!("EngineMsg: {}, {}", (cycles - orig_time), (send_time - orig_time));

              },
              Err(_) => {
                  rprintln!("Log: error")
              }
          }
        }
    }

    #[task(local = [state], shared=[], priority=2)]
    async fn sim(cx: sim::Context) {
        loop {
            if *cx.local.state {
                *cx.local.state = false;
            } else {
                *cx.local.state = true;
            }
            #[allow(deprecated)]
            let cycles = cortex_m::peripheral::DWT::get_cycle_count();
            decode::spawn(Trigger{time: cycles, trigger: 0}).ok();
            Systick::delay(1000.millis()).await;
        }
    }
}
