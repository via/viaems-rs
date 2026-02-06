use crate::interface;
use crate::table_editor::{Table2dEditor, Table1dEditor};


fn render_single_optional_value_input<T: emath::Numeric + Default>(ui: &mut egui::Ui, reference: &Option<T>, field: &mut Option<T>, name: &str) { 

    let reference = *reference.clone().get_or_insert_default();
    let field = field.get_or_insert_default();
    render_single_value_input(ui, &reference, field, name);
}

fn render_single_value_input<T: emath::Numeric>(ui: &mut egui::Ui, reference: &T, field: &mut T, name: &str) { 
    ui.horizontal(|ui| {
        ui.label(name);
        ui.add(egui::DragValue::new(field)
            .update_while_editing(false));
        if reference != field {
            if ui.button(egui::RichText::new("↺").color(egui::Color32::RED)).clicked() {
                *field = reference.clone();
            }
        }
    });
}

fn render_enum_selector<T: PartialEq + Copy>(ui: &mut egui::Ui, reference: Option<i32>, field: &mut Option<i32>, name: &str,
    entries: &[(T, &str)]) 
  where i32: TryFrom<T>
{
    ui.horizontal(|ui| {
        ui.label(name);

        let mut selected_text : Option<&str> = None;
        for (i, s) in entries.iter() {
            let i_as_i32 = i32::try_from(*i).unwrap_or(0);

            if *field.get_or_insert_default() == i_as_i32 {
                selected_text = Some(s);
            }
        }
        let selected_text = selected_text.unwrap_or(entries[0].1);

        let field_as_i32 = field.get_or_insert_default();
        egui::ComboBox::from_id_salt(name).selected_text(selected_text).show_ui(ui, |ui| {
            for (i, s) in entries.iter() {
                let i_as_i32 = i32::try_from(*i).unwrap_or(0);
                ui.selectable_value(field_as_i32, i_as_i32, *s);
            }
        });
        if reference.unwrap_or_default() != *field_as_i32 {
            if ui.button(egui::RichText::new("↺").color(egui::Color32::RED)).clicked() {
                *field = reference;
            }
        }
    });
}

fn render_table1d(ui: &mut egui::Ui, 
                  reference: &interface::configuration::Table1d,
                  field: &mut interface::configuration::Table1d,
                  name: &str) {
    egui::CollapsingHeader::new(red_if_changed(reference, field, name)).show(ui, |ui| {
        Table1dEditor::new().show(ui, reference, field);
    });

}

fn render_table2d(ui: &mut egui::Ui, 
                  reference: &interface::configuration::Table2d,
                  field: &mut interface::configuration::Table2d,
                  name: &str) {
    egui::CollapsingHeader::new(red_if_changed(reference, field, name)).show(ui, |ui| {
        Table2dEditor::new().show(ui, reference, field);
    });

}

fn red_if_changed<T: PartialEq>(reference: &T, local: &T, text: &str) -> egui::RichText {
    if reference == local {
        egui::RichText::new(text)
    } else {
        egui::RichText::new(text).color(egui::Color32::RED)
    }
}

fn render_sensor(ui: &mut egui::Ui, name: &str, live_sensor: &Option<interface::configuration::Sensor>, sensor: &mut Option<interface::configuration::Sensor>) {
    let sensor = sensor.get_or_insert_default();
    let sensor_ref = live_sensor.clone().unwrap_or_default();

    egui::CollapsingHeader::new(red_if_changed(&sensor_ref, sensor, name)).show(ui, |ui| {
        render_enum_selector(ui, 
            sensor_ref.source, 
            &mut sensor.source,
            "Source",
            &[
            (interface::configuration::SensorSource::SourceNone, "None"),
            (interface::configuration::SensorSource::SourceAdc, "Adc"),
            (interface::configuration::SensorSource::SourceFreq, "Frequency"),
            (interface::configuration::SensorSource::SourcePulsewidth, "Pulsewidth"),
            (interface::configuration::SensorSource::SourceConst, "Constant"),
            ]);

        render_enum_selector(ui, 
            sensor_ref.method, 
            &mut sensor.method,
            "Method",
            &[
            (interface::configuration::SensorMethod::MethodLinear, "Linear"),
            (interface::configuration::SensorMethod::MethodLinearWindowed, "Linear windows"),
            (interface::configuration::SensorMethod::MethodThermistor, "Thermistor"),
            ]);


        render_single_optional_value_input(ui, &sensor_ref.pin, &mut sensor.pin, "Pin");
        render_single_optional_value_input(ui, &sensor_ref.lag, &mut sensor.lag, "Lag filter");

        ui.separator();

        if sensor.source() == interface::configuration::SensorSource::SourceConst {
            let cc = sensor.const_config.get_or_insert_default();
            let cc_ref = sensor_ref.const_config.unwrap_or_default();
            render_single_value_input(ui, &cc_ref.fixed_value, &mut cc.fixed_value, "Fixed Value");
        } else {
            if (sensor.method() == interface::configuration::SensorMethod::MethodLinear) ||
                (sensor.method() == interface::configuration::SensorMethod::MethodLinearWindowed) {
                    let lc = sensor.linear_config.get_or_insert_default();
                    let lc_ref = sensor_ref.linear_config.unwrap_or_default();
                    render_single_value_input(ui, &lc_ref.input_min, &mut lc.input_min, "Input lower bound");
                    render_single_value_input(ui, &lc_ref.input_max, &mut lc.input_max, "Input upper bound");
                    render_single_value_input(ui, &lc_ref.output_min, &mut lc.output_min, "Output lower bound");
                    render_single_value_input(ui, &lc_ref.output_max, &mut lc.output_max, "Output upper bound");

                    if sensor.method() == interface::configuration::SensorMethod::MethodLinearWindowed {
                        ui.separator();
                        let wc = sensor.window_config.get_or_insert_default();
                        let wc_ref = sensor_ref.window_config.unwrap_or_default();

                        render_single_value_input(ui, &wc_ref.opening, &mut wc.opening, "Window Capture Opening");
                        render_single_value_input(ui, &wc_ref.count, &mut wc.count, "Windows per cycle");
                        render_single_value_input(ui, &wc_ref.offset, &mut wc.offset, "Window Offset");
                    }
                } else if sensor.method() == interface::configuration::SensorMethod::MethodThermistor {
                    let tc = sensor.thermistor_config.get_or_insert_default();
                    let tc_ref = sensor_ref.thermistor_config.unwrap_or_default();

                    render_single_value_input(ui, &tc_ref.bias, &mut tc.bias, "Bias (Ohms)");
                    render_single_value_input(ui, &tc_ref.a, &mut tc.a, "A");
                    render_single_value_input(ui, &tc_ref.b, &mut tc.b, "B");
                    render_single_value_input(ui, &tc_ref.c, &mut tc.c, "C");
                }

            let fc = sensor.fault_config.get_or_insert_default();
            let fc_ref = sensor_ref.fault_config.unwrap_or_default();

            ui.separator();

            render_single_value_input(ui, &fc_ref.min, &mut fc.min, "Minimum Input");
            render_single_value_input(ui, &fc_ref.max, &mut fc.max, "Maximum Input");
            render_single_value_input(ui, &fc_ref.value, &mut fc.value, "Fallback value");

        }

    });
}

fn render_knock_sensor(ui: &mut egui::Ui, name: &str, live_sensor: &Option<interface::configuration::KnockSensor>, sensor: &mut Option<interface::configuration::KnockSensor>) {
    let sensor = sensor.get_or_insert_default();
    let sensor_ref = live_sensor.clone().unwrap_or_default();

    egui::CollapsingHeader::new(red_if_changed(&sensor_ref, sensor, name)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(red_if_changed(&sensor_ref.enabled, &sensor.enabled, "Enabled"));
            let enabled = sensor.enabled.get_or_insert_default();
            ui.checkbox(enabled, "");
        });
        render_single_optional_value_input(ui, &sensor_ref.frequency, &mut sensor.frequency, "Frequency");
        render_single_optional_value_input(ui, &sensor_ref.threshold, &mut sensor.threshold, "Threshold");

    });

}

pub fn render_config_pane(ui: &mut egui::Ui, live_config: &interface::Configuration, local_config: &mut interface::Configuration) {
    egui::ScrollArea::vertical().show(ui, |ui| {

        egui::CollapsingHeader::new(red_if_changed(&live_config.outputs, &local_config.outputs, "Outputs"))
            .show(ui, |ui| {
            egui::Grid::new("outputlist")
                .num_columns(4)
                .striped(true)
                .show(ui, |ui| {
                    ui.label("Type");
                    ui.label("Pin");
                    ui.label("Angle");
                    ui.label("Inverted");
                    ui.end_row();

                    for (idx, output) in local_config.outputs.iter_mut().enumerate() {
                        let mut selvalue = output.r#type();
                        let seltext = match selvalue {
                            interface::configuration::output::OutputType::OutputDisabled => "Disabled",
                            interface::configuration::output::OutputType::OutputFuel => "Fuel",
                            interface::configuration::output::OutputType::OutputIgnition => "Ignition",
                        };

                        egui::ComboBox::from_id_salt(idx)
                            .selected_text(seltext)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut selvalue,
                                    interface::configuration::output::OutputType::OutputDisabled,
                                    "Disabled");
                                ui.selectable_value(&mut selvalue,
                                    interface::configuration::output::OutputType::OutputFuel,
                                    "Fuel");
                                ui.selectable_value(&mut selvalue,
                                    interface::configuration::output::OutputType::OutputIgnition,
                                    "Ignition");

                            });
                        output.r#type = Some(selvalue as i32);

                        ui.add(egui::DragValue::new(output.pin.get_or_insert_default()).update_while_editing(false));
                        ui.add(egui::DragValue::new(output.angle.get_or_insert_default()).update_while_editing(false));

                        let mut inverted = output.inverted();
                        ui.checkbox(&mut inverted, "");
                        output.inverted = Some(inverted);

                        if idx < live_config.outputs.len() && *output != live_config.outputs[idx] {
                            if ui.button(egui::RichText::new("↺").color(egui::Color32::RED)).clicked() {
                                *output = live_config.outputs[idx].clone();
                            }
                        }


                        ui.end_row();
                    }
                });
        });

        egui::CollapsingHeader::new(red_if_changed(&live_config.triggers, &local_config.triggers, "Triggers")).show(ui, |ui| {
            egui::Grid::new("inputlist")
                .num_columns(2)
                .striped(true)
                .show(ui, |ui| {
                    ui.label("Type");
                    ui.label("Edge");
                    ui.end_row();

                    for (idx, trigger) in local_config.triggers.iter_mut().enumerate() {
                        let mut typevalue = trigger.r#type();
                        let typetext = match typevalue {
                            interface::configuration::InputType::InputDisabled => "Disabled",
                            interface::configuration::InputType::InputTrigger => "Trigger",
                            interface::configuration::InputType::InputSync => "Sync",
                            interface::configuration::InputType::InputFreq => "Frequency",
                        };

                        egui::ComboBox::from_id_salt(idx)
                            .selected_text(typetext)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut typevalue,
                                    interface::configuration::InputType::InputDisabled,
                                    "Disabled");
                                ui.selectable_value(&mut typevalue,
                                    interface::configuration::InputType::InputTrigger,
                                    "Trigger");
                                ui.selectable_value(&mut typevalue,
                                    interface::configuration::InputType::InputSync,
                                    "Sync");
                                ui.selectable_value(&mut typevalue,
                                    interface::configuration::InputType::InputFreq,
                                    "Frequency");

                            });
                        trigger.r#type = Some(typevalue as i32);

                        let mut edgevalue = trigger.edge();
                        let edgetext = match edgevalue {
                            interface::configuration::InputEdge::EdgeRising => "Rising",
                            interface::configuration::InputEdge::EdgeFalling => "Falling",
                            interface::configuration::InputEdge::EdgeBoth => "Both",
                        };


                        egui::ComboBox::from_id_salt(16+idx)
                            .selected_text(edgetext)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut edgevalue,
                                    interface::configuration::InputEdge::EdgeRising,
                                    "Rising");
                                ui.selectable_value(&mut edgevalue,
                                    interface::configuration::InputEdge::EdgeFalling,
                                    "Falling");
                                ui.selectable_value(&mut edgevalue,
                                    interface::configuration::InputEdge::EdgeBoth,
                                    "Both");

                            });
                        trigger.edge = Some(edgevalue as i32);

                        if idx < live_config.triggers.len() &&  *trigger != live_config.triggers[idx] {
                            if ui.button(egui::RichText::new("↺").color(egui::Color32::RED)).clicked() {
                                *trigger = live_config.triggers[idx].clone();
                            }
                        }
                        ui.end_row();
                    }
                });

        });

        let sensors = local_config.sensors.get_or_insert_default();
        let sensors_ref = live_config.sensors.clone().unwrap_or_default();
        egui::CollapsingHeader::new(red_if_changed(&sensors_ref, sensors, "Sensors")).show(ui, |ui| {
            render_sensor(ui, "MAP", &sensors_ref.map, &mut sensors.map);
            render_sensor(ui, "AAP", &sensors_ref.aap, &mut sensors.aap);
            render_sensor(ui, "BRV", &sensors_ref.brv, &mut sensors.brv);
            render_sensor(ui, "CLT", &sensors_ref.clt, &mut sensors.clt);
            render_sensor(ui, "IAT", &sensors_ref.iat, &mut sensors.iat);
            render_sensor(ui, "TPS", &sensors_ref.tps, &mut sensors.tps);
            render_sensor(ui, "EGO", &sensors_ref.ego, &mut sensors.ego);
            render_sensor(ui, "FRP", &sensors_ref.frp, &mut sensors.frp);
            render_sensor(ui, "FRT", &sensors_ref.frt, &mut sensors.frt);
            render_sensor(ui, "ETH", &sensors_ref.eth, &mut sensors.eth);

            render_knock_sensor(ui, "Knock 1", &sensors_ref.knock1, &mut sensors.knock1);
            render_knock_sensor(ui, "Knock 2", &sensors_ref.knock2, &mut sensors.knock2);
        });

        
        let ignition = local_config.ignition.get_or_insert_default();
        let ignition_ref = live_config.ignition.clone().unwrap_or_default();
        egui::CollapsingHeader::new(red_if_changed(&ignition_ref, ignition, "Ignition")).show(ui, |ui| {


            render_enum_selector(ui, 
                ignition_ref.r#type, 
                &mut ignition.r#type, 
                "Dwell Type",
                &[
                (interface::configuration::ignition::DwellType::DwellFixedDuty, "Fixed Duty"),
                (interface::configuration::ignition::DwellType::DwellFixedTime, "Fixed Time"),
                (interface::configuration::ignition::DwellType::DwellBrv, "Battery Voltage"),
                ]);

            match ignition.r#type() {
                interface::configuration::ignition::DwellType::DwellFixedDuty => {
                    render_single_optional_value_input(ui, &ignition_ref.fixed_duty, &mut ignition.fixed_duty, "Dwell Duty (%)");
                }
                interface::configuration::ignition::DwellType::DwellFixedTime => {
                    render_single_optional_value_input(ui, &ignition_ref.fixed_dwell, &mut ignition.fixed_dwell, "Dwell Time (uS)");
                }
                interface::configuration::ignition::DwellType::DwellBrv => {
                    let dwell = ignition.dwell.get_or_insert_default();
                    let dwell_ref = ignition_ref.dwell.unwrap_or_default();
                    render_table1d(ui, &dwell_ref, dwell, "Dwell");
                }
            }

            render_single_optional_value_input(ui, &ignition_ref.ignitions_per_cycle, &mut ignition.ignitions_per_cycle, "Ignitions per cycle");
            render_single_optional_value_input(ui, &ignition_ref.min_coil_cooldown_us, &mut ignition.min_coil_cooldown_us, "Coil cooldown (uS)");
            render_single_optional_value_input(ui, &ignition_ref.min_dwell_us, &mut ignition.min_dwell_us, "Minimum Dwell (uS)");

            let timing = ignition.timing.get_or_insert_default();
            let timing_ref = ignition_ref.timing.unwrap_or_default();
            render_table2d(ui, &timing_ref, timing, "Timing");
        });

        let fueling = local_config.fueling.get_or_insert_default();
        let fueling_ref = live_config.fueling.clone().unwrap_or_default();
        egui::CollapsingHeader::new(red_if_changed(&fueling_ref, fueling, "Fueling")).show(ui, |ui| {
            render_single_optional_value_input(ui, &fueling_ref.fuel_pump_pin, &mut fueling.fuel_pump_pin, "Fuelpump pin");
            render_single_optional_value_input(ui, &fueling_ref.cylinder_cc, &mut fueling.cylinder_cc, "Cylinder CC");
            render_single_optional_value_input(ui, &fueling_ref.fuel_density, &mut fueling.fuel_density, "Fuel Density");
            render_single_optional_value_input(ui, &fueling_ref.fuel_stoich_ratio, &mut fueling.fuel_stoich_ratio, "Fuel Stoich Ratio");
            render_single_optional_value_input(ui, &fueling_ref.injections_per_cycle, &mut fueling.injections_per_cycle, "Injections per cycle");
            render_single_optional_value_input(ui, &fueling_ref.injector_cc, &mut fueling.injector_cc, "Injector CC/min");
            render_single_optional_value_input(ui, &fueling_ref.max_duty_cycle, &mut fueling.max_duty_cycle, "Max Duty Cycle (%)");

            let ce = fueling.crank_enrich.get_or_insert_default();
            let ce_ref = fueling_ref.crank_enrich.clone().unwrap_or_default();
            egui::CollapsingHeader::new(red_if_changed(&ce_ref, ce, "Cranking Enrichment")).default_open(true).show(ui, |ui| {
                render_single_optional_value_input(ui, &ce_ref.cranking_rpm, &mut ce.cranking_rpm, "Upper Cranking RPM");
                render_single_optional_value_input(ui, &ce_ref.cranking_temp, &mut ce.cranking_temp, "Upper Temp Threshold");
                render_single_optional_value_input(ui, &ce_ref.enrich_amt, &mut ce.enrich_amt, "Multiplier");
            });

            let pwc = fueling.pulse_width_compensation.get_or_insert_default();
            let pwc_ref = fueling_ref.pulse_width_compensation.unwrap_or_default();
            render_table1d(ui, &pwc_ref, pwc, "Pulse Width Compensation");

            let idt = fueling.injector_dead_time.get_or_insert_default();
            let idt_ref = fueling_ref.injector_dead_time.unwrap_or_default();
            render_table1d(ui, &idt_ref, idt, "Injector Dead Time");

            let ete = fueling.engine_temp_enrichment.get_or_insert_default();
            let ete_ref = fueling_ref.engine_temp_enrichment.unwrap_or_default();
            render_table2d(ui, &ete_ref, ete, "Engine Temp Enrichment");

            let ve = fueling.ve.get_or_insert_default();
            let ve_ref = fueling_ref.ve.unwrap_or_default();
            render_table2d(ui, &ve_ref, ve, "VE");

            let lambda = fueling.commanded_lambda.get_or_insert_default();
            let lambda_ref = fueling_ref.commanded_lambda.unwrap_or_default();
            render_table2d(ui, &lambda_ref, lambda, "Lambda");

            let tipin_amt = fueling.tipin_enrich_amount.get_or_insert_default();
            let tipin_amt_ref = fueling_ref.tipin_enrich_amount.unwrap_or_default();
            render_table2d(ui, &tipin_amt_ref, tipin_amt, "Tipin Amount");

            let tipin_duration = fueling.tipin_enrich_duration.get_or_insert_default();
            let tipin_duration_ref = fueling_ref.tipin_enrich_duration.unwrap_or_default();
            render_table1d(ui, &tipin_duration_ref, tipin_duration, "Tipin Duration");

        });

        let decoder = local_config.decoder.get_or_insert_default();
        let decoder_ref = live_config.decoder.clone().unwrap_or_default();
        egui::CollapsingHeader::new(red_if_changed(&decoder_ref, decoder, "Decoder")).show(ui, |ui| {
            render_enum_selector(ui, 
                decoder_ref.trigger_type,
                &mut decoder.trigger_type,
                "Trigger wheel type",
                &[
                (interface::configuration::TriggerType::DecoderDisabled, "Disabled"),
                (interface::configuration::TriggerType::EvenTeeth, "Even Teeth"),
                (interface::configuration::TriggerType::EvenTeethPlusCamsync, "Even Teeth and Cam"),
                (interface::configuration::TriggerType::MissingTooth, "Missing Tooth"),
                (interface::configuration::TriggerType::MissingToothPlusCamsync, "Missing Tooth and Cam"),
                ]);

            render_single_optional_value_input(ui, &decoder_ref.degrees_per_trigger, &mut decoder.degrees_per_trigger, "Degrees per trigger");
            render_single_optional_value_input(ui, &decoder_ref.max_tooth_variance, &mut decoder.max_tooth_variance, "Max tooth variance");
            render_single_optional_value_input(ui, &decoder_ref.min_rpm, &mut decoder.min_rpm, "Minimum RPM");
            render_single_optional_value_input(ui, &decoder_ref.num_triggers, &mut decoder.num_triggers, "Trigger count");
            render_single_optional_value_input(ui, &decoder_ref.offset, &mut decoder.offset, "Offset");
        });

        let rpm_cut = local_config.rpm_cut.get_or_insert_default();
        let rpm_cut_ref = live_config.rpm_cut.clone().unwrap_or_default();
        egui::CollapsingHeader::new(red_if_changed(&rpm_cut_ref, &rpm_cut, "Rpm Cut")).show(ui, |ui| {
            render_single_optional_value_input(ui, &rpm_cut_ref.rpm_limit_start, &mut rpm_cut.rpm_limit_start, "RPM Lower Hysteresis");
            render_single_optional_value_input(ui, &rpm_cut_ref.rpm_limit_stop, &mut rpm_cut.rpm_limit_stop, "RPM Upper Hysteresis");
        });

        let cel = local_config.cel.get_or_insert_default();
        let cel_ref = live_config.cel.clone().unwrap_or_default();
        egui::CollapsingHeader::new(red_if_changed(&cel_ref, &cel, "Check Engine Light")).show(ui, |ui| {
            render_single_optional_value_input(ui, &cel_ref.pin, &mut cel.pin, "Pin");
            render_single_optional_value_input(ui, &cel_ref.lean_boost_ego, &mut cel.lean_boost_ego, "Max EGO-in-boost");
            render_single_optional_value_input(ui, &cel_ref.lean_boost_map_enable, &mut cel.lean_boost_map_enable, "EGO-in-boost MAP threshold (kpa)");
        });

        let boost = local_config.boost_control.get_or_insert_default();
        let boost_ref = live_config.boost_control.clone().unwrap_or_default();
        egui::CollapsingHeader::new(red_if_changed(&boost_ref, &boost, "Boost Control")).show(ui, |ui| {
            render_single_optional_value_input(ui, &boost_ref.pin, &mut boost.pin, "Pin");
            render_single_optional_value_input(ui, &boost_ref.control_threshold_map, &mut boost.control_threshold_map, "Lower MAP threshold (kpa)");
            render_single_optional_value_input(ui, &boost_ref.control_threshold_tps, &mut boost.control_threshold_tps, "Lower TPS threshold (%)");
            render_single_optional_value_input(ui, &boost_ref.enable_threshold_map, &mut boost.enable_threshold_map, "Enable MAP threshold (kpa)");
            render_single_optional_value_input(ui, &boost_ref.overboost_map, &mut boost.overboost_map, "Overboost limit (kpa)");

            let pwm = boost.pwm_vs_rpm.get_or_insert_default();
            let pwm_ref = boost_ref.pwm_vs_rpm.unwrap_or_default();
            render_table1d(ui, &pwm_ref, pwm, "PWM vs RPM");

        });
    });
}
