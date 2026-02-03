use crate::interface;
use crate::table_editor::{Table2dEditor, Table1dEditor};

fn render_single_value_input<T: emath::Numeric>(ui: &mut egui::Ui, field: &mut T, name: &str) { 
    ui.horizontal(|ui| {
        ui.label(name);
        ui.add(egui::DragValue::new(field)
            .update_while_editing(false));
            });
}

fn render_sensor(ui: &mut egui::Ui, name: &str, sensor: &mut Option<interface::configuration::Sensor>) {
    let sensor = sensor.get_or_insert_default();

    egui::CollapsingHeader::new(name.to_string()).show(ui, |ui| {
        let mut sourcevalue = sensor.source();
        let sourcetext = match sourcevalue {
            interface::configuration::SensorSource::SourceNone => "None",
            interface::configuration::SensorSource::SourceAdc => "Adc",
            interface::configuration::SensorSource::SourceFreq => "Frequency",
            interface::configuration::SensorSource::SourcePulsewidth => "Pulsewidth",
            interface::configuration::SensorSource::SourceConst => "Constant",
        };

        ui.horizontal(|ui| {
            ui.label("Source");
            egui::ComboBox::from_id_salt("Source")
                .selected_text(sourcetext)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut sourcevalue,
                        interface::configuration::SensorSource::SourceNone,
                        "None");
                    ui.selectable_value(&mut sourcevalue,
                        interface::configuration::SensorSource::SourceAdc,
                        "Adc");
                    ui.selectable_value(&mut sourcevalue,
                        interface::configuration::SensorSource::SourceFreq,
                        "Frequency");
                    ui.selectable_value(&mut sourcevalue,
                        interface::configuration::SensorSource::SourcePulsewidth,
                        "Pulsewidth");
                    ui.selectable_value(&mut sourcevalue,
                        interface::configuration::SensorSource::SourceConst,
                        "Constant");

                });
            sensor.source = Some(sourcevalue as i32);
        });

        let mut methodvalue = sensor.method();
        let methodtext = match methodvalue {
            interface::configuration::SensorMethod::MethodLinear => "Linear",
            interface::configuration::SensorMethod::MethodLinearWindowed => "Linear windowed",
            interface::configuration::SensorMethod::MethodThermistor => "Thermistor",
        };

        ui.horizontal(|ui| {
            ui.label("Method");
            egui::ComboBox::from_id_salt("Method")
                .selected_text(methodtext)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut methodvalue,
                        interface::configuration::SensorMethod::MethodLinear,
                        "Linear");
                    ui.selectable_value(&mut methodvalue,
                        interface::configuration::SensorMethod::MethodLinearWindowed,
                        "Linear windowed");
                    ui.selectable_value(&mut methodvalue,
                        interface::configuration::SensorMethod::MethodThermistor,
                        "Thermistor");

                });
            sensor.method = Some(methodvalue as i32);
        });

        render_single_value_input(ui, sensor.pin.get_or_insert_default(), "Pin");
        render_single_value_input(ui, sensor.lag.get_or_insert_default(), "Lag filter");

        ui.separator();

        if sourcevalue == interface::configuration::SensorSource::SourceConst {
            let mut cc = sensor.const_config.unwrap_or_default();
            render_single_value_input(ui, &mut cc.fixed_value, "Fixed Value");
            sensor.const_config = Some(cc);
        } else {
            if (methodvalue == interface::configuration::SensorMethod::MethodLinear) ||
               (methodvalue == interface::configuration::SensorMethod::MethodLinearWindowed) {
                let mut lc = sensor.linear_config.unwrap_or_default();

                ui.horizontal(|ui| {
                    ui.label("Input Range");
                    ui.add(egui::DragValue::new(&mut lc.input_min).update_while_editing(false));
                    ui.add(egui::DragValue::new(&mut lc.input_max).update_while_editing(false));


                });

                ui.horizontal(|ui| {
                    ui.label("Output Range");
                    ui.add(egui::DragValue::new(&mut lc.output_min).update_while_editing(false));
                    ui.add(egui::DragValue::new(&mut lc.output_max).update_while_editing(false));
                });

                sensor.linear_config = Some(lc);

                if methodvalue == interface::configuration::SensorMethod::MethodLinearWindowed {
                    ui.separator();
                    let mut wc = sensor.window_config.unwrap_or_default();

                    render_single_value_input(ui, &mut wc.capture_width, "Window Capture Opening");
                    render_single_value_input(ui, &mut wc.total_width, "Window Total Width");
                    render_single_value_input(ui, &mut wc.offset, "Window Offset");

                    sensor.window_config = Some(wc);

                }
        } else if methodvalue == interface::configuration::SensorMethod::MethodThermistor {
            let mut tc = sensor.thermistor_config.unwrap_or_default();

            render_single_value_input(ui, &mut tc.bias, "Bias (Ohms)");
            render_single_value_input(ui, &mut tc.a, "A");
            render_single_value_input(ui, &mut tc.b, "B");
            render_single_value_input(ui, &mut tc.c, "C");

            sensor.thermistor_config = Some(tc);
        }
            let mut fc = sensor.fault_config.unwrap_or_default();

            ui.separator();

            render_single_value_input(ui, &mut fc.min, "Minimum Input");
            render_single_value_input(ui, &mut fc.max, "Maximum Input");
            render_single_value_input(ui, &mut fc.value, "Fallback value");

        }

    });
}

pub fn render_config_pane(ui: &mut egui::Ui, config: &mut interface::Configuration) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        egui::CollapsingHeader::new("Outputs").show(ui, |ui| {
            egui::Grid::new("outputlist")
                .num_columns(4)
                .striped(true)
                .show(ui, |ui| {
                    ui.label("Type");
                    ui.label("Pin");
                    ui.label("Angle");
                    ui.label("Inverted");
                    ui.end_row();

                    for (idx, output) in config.outputs.iter_mut().enumerate() {
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

                        ui.end_row();
                    }
                });
        });

        egui::CollapsingHeader::new("Inputs").show(ui, |ui| {
            egui::Grid::new("inputlist")
                .num_columns(2)
                .striped(true)
                .show(ui, |ui| {
                    ui.label("Type");
                    ui.label("Edge");
                    ui.end_row();

                    for (idx, input) in config.triggers.iter_mut().enumerate() {
                        let mut typevalue = input.r#type();
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
                        input.r#type = Some(typevalue as i32);

                        let mut edgevalue = input.edge();
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
                        input.edge = Some(edgevalue as i32);
                        ui.end_row();
                    }
                });

        });

        egui::CollapsingHeader::new("Sensors").show(ui, |ui| {
            let sensors = config.sensors.get_or_insert_default();
            render_sensor(ui, "MAP", &mut sensors.map);
            render_sensor(ui, "AAP", &mut sensors.aap);
            render_sensor(ui, "BRV", &mut sensors.brv);
            render_sensor(ui, "CLT", &mut sensors.clt);
            render_sensor(ui, "IAT", &mut sensors.iat);
            render_sensor(ui, "TPS", &mut sensors.tps);
            render_sensor(ui, "EGO", &mut sensors.ego);
            render_sensor(ui, "FRP", &mut sensors.frp);
            render_sensor(ui, "FRT", &mut sensors.frt);
            render_sensor(ui, "ETH", &mut sensors.eth);
        });

        egui::CollapsingHeader::new("Ignition").default_open(true).show(ui, |ui| {
            let ignition = &mut config.ignition.get_or_insert_default();

            let mut dwelltype = ignition.r#type();
            let dwelltext = match dwelltype {
                interface::configuration::ignition::DwellType::DwellFixedDuty => "Fixed Duty",
                interface::configuration::ignition::DwellType::DwellFixedTime => "Fixed Time",
                interface::configuration::ignition::DwellType::DwellBrv => "Battery Voltage",
            };


            ui.horizontal(|ui| {
                ui.label("Dwell Source");
                egui::ComboBox::from_id_salt("dwelltype")
                    .selected_text(dwelltext)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut dwelltype,
                            interface::configuration::ignition::DwellType::DwellFixedDuty,
                            "Fixed Duty");
                        ui.selectable_value(&mut dwelltype,
                            interface::configuration::ignition::DwellType::DwellFixedTime,
                            "Fixed Time");
                        ui.selectable_value(&mut dwelltype,
                            interface::configuration::ignition::DwellType::DwellBrv,
                            "Battery Voltage");

                    });
                ignition.r#type = Some(dwelltype as i32);
            });

            match dwelltype {
                interface::configuration::ignition::DwellType::DwellFixedDuty => {
                    ui.horizontal(|ui| {
                        ui.label("Dwell Duty %");
                        ui.add(egui::DragValue::new(ignition.fixed_duty.get_or_insert_default())
                            .update_while_editing(false));
                        });
                }
                interface::configuration::ignition::DwellType::DwellFixedTime => {
                    ui.horizontal(|ui| {
                        ui.label("Dwell Time (uS)");
                        ui.add(egui::DragValue::new(ignition.fixed_dwell.get_or_insert_default())
                            .update_while_editing(false));
                            });
                }
                interface::configuration::ignition::DwellType::DwellBrv => {
                    egui::CollapsingHeader::new("Dwell").default_open(false).show(ui, |ui| {
                        Table1dEditor::new().show(ui, ignition.dwell.get_or_insert_default());
                        
                    });
                    
                }
            }

            ui.horizontal(|ui| {
                ui.label("Ignitions per cycle");
                ui.add(egui::DragValue::new(ignition.ignitions_per_cycle.get_or_insert_default())
                    .update_while_editing(false)
                    .range(1..=8));
            });

            ui.horizontal(|ui| {
                ui.label("Coil cooldown (uS)");
                ui.add(egui::DragValue::new(ignition.min_coil_cooldown_us.get_or_insert_default())
                    .update_while_editing(false));
            });

            ui.horizontal(|ui| {
                ui.label("Minimum Dwell");
                ui.add(egui::DragValue::new(ignition.min_dwell_us.get_or_insert_default())
                    .update_while_editing(false));

            });

            egui::CollapsingHeader::new("Timing").show(ui, |ui| {
              Table2dEditor::new().show(ui, ignition.timing.get_or_insert_default());
            });
        });

        egui::CollapsingHeader::new("Fueling").default_open(false).show(ui, |ui| {
            let fueling = config.fueling.get_or_insert_default();

            render_single_value_input(ui, fueling.fuel_pump_pin.get_or_insert_default(), "Fuelpump pin");
            render_single_value_input(ui, fueling.cylinder_cc.get_or_insert_default(), "Cylinder CC");
            render_single_value_input(ui, fueling.fuel_density.get_or_insert_default(), "Fuel Density");
            render_single_value_input(ui, fueling.fuel_stoich_ratio.get_or_insert_default(), "Fuel Stoich Ratio");
            render_single_value_input(ui, fueling.injections_per_cycle.get_or_insert_default(), "Injections per cycle");
            render_single_value_input(ui, fueling.injector_cc.get_or_insert_default(), "Injector CC/min");
            render_single_value_input(ui, fueling.max_duty_cycle.get_or_insert_default(), "Max Duty Cycle (%)");

            egui::CollapsingHeader::new("Cranking Enrichment").default_open(true).show(ui, |ui| {
                let ce = fueling.crank_enrich.get_or_insert_default();
                render_single_value_input(ui, ce.cranking_rpm.get_or_insert_default(), "Upper Cranking RPM");
                render_single_value_input(ui, ce.cranking_temp.get_or_insert_default(), "Upper Temp Threshold");
                render_single_value_input(ui, ce.enrich_amt.get_or_insert_default(), "Multiplier");
            });

            egui::CollapsingHeader::new("Pulse Width Compensation").show(ui, |ui| {
                Table1dEditor::new().show(ui, fueling.pulse_width_compensation.get_or_insert_default());
            });

            egui::CollapsingHeader::new("Injector Dead Time").show(ui, |ui| {
                Table1dEditor::new().show(ui, fueling.injector_dead_time.get_or_insert_default());
            });

            egui::CollapsingHeader::new("Engine Temp Enrichment").show(ui, |ui| {
                Table2dEditor::new().show(ui, fueling.engine_temp_enrichment.get_or_insert_default());
            });

            egui::CollapsingHeader::new("VE").show(ui, |ui| {
                Table2dEditor::new().show(ui, fueling.ve.get_or_insert_default());
            });

            egui::CollapsingHeader::new("Lambda").show(ui, |ui| {
                Table2dEditor::new().show(ui, fueling.commanded_lambda.get_or_insert_default());
            });

            egui::CollapsingHeader::new("Tipin Amount").show(ui, |ui| {
                Table2dEditor::new().show(ui, fueling.tipin_enrich_amount.get_or_insert_default());
            });

            egui::CollapsingHeader::new("Tipin Duration").show(ui, |ui| {
                Table1dEditor::new().show(ui, fueling.tipin_enrich_duration.get_or_insert_default());
            });
        });

        egui::CollapsingHeader::new("Decoder").default_open(true).show(ui, |ui| {
            let decoder = config.decoder.get_or_insert_default();

            ui.horizontal(|ui| {
                ui.label("Trigger wheel type");

                let mut decoder_type = decoder.trigger_type();
                let decoder_text = match decoder_type {
                    interface::configuration::TriggerType::DecoderDisabled => "Disabled",
                    interface::configuration::TriggerType::EvenTeeth => "Even Teeth",
                    interface::configuration::TriggerType::EvenTeethPlusCamsync => "Even Teeth and Cam",
                    interface::configuration::TriggerType::MissingTooth => "Missing Tooth",
                    interface::configuration::TriggerType::MissingToothPlusCamsync => "Missing Tooth and Cam",
                };

                egui::ComboBox::from_id_salt("decodertype")
                    .selected_text(decoder_text)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut decoder_type,
                            interface::configuration::TriggerType::DecoderDisabled,
                            "Disabled");
                        ui.selectable_value(&mut decoder_type,
                            interface::configuration::TriggerType::EvenTeeth,
                            "Even Teeth");
                        ui.selectable_value(&mut decoder_type,
                            interface::configuration::TriggerType::EvenTeethPlusCamsync,
                            "Even Teeth and Cam");
                        ui.selectable_value(&mut decoder_type,
                            interface::configuration::TriggerType::MissingTooth,
                            "Missing Tooth");
                        ui.selectable_value(&mut decoder_type,
                            interface::configuration::TriggerType::MissingToothPlusCamsync,
                            "Missing Tooth and Cam");

                    });
                decoder.trigger_type = Some(decoder_type as i32);
            });

            render_single_value_input(ui, decoder.degrees_per_trigger.get_or_insert_default(), "Degrees per trigger");
            render_single_value_input(ui, decoder.max_tooth_variance.get_or_insert_default(), "Max tooth variance");
            render_single_value_input(ui, decoder.min_rpm.get_or_insert_default(), "Minimum RPM");
            render_single_value_input(ui, decoder.num_triggers.get_or_insert_default(), "Trigger count");
            render_single_value_input(ui, decoder.offset.get_or_insert_default(), "Offset");
        });

        egui::CollapsingHeader::new("Misc").default_open(true).show(ui, |ui| {
            egui::CollapsingHeader::new("Rpm Cut").default_open(true).show(ui, |ui| {
                let rpm_cut = config.rpm_cut.get_or_insert_default();
                render_single_value_input(ui, rpm_cut.rpm_limit_start.get_or_insert_default(), "RPM Lower Hysteresis");
                render_single_value_input(ui, rpm_cut.rpm_limit_stop.get_or_insert_default(), "RPM Upper Hysteresis");
            });

            egui::CollapsingHeader::new("Check Engine Light").default_open(true).show(ui, |ui| {
                let cel = config.cel.get_or_insert_default();
                render_single_value_input(ui, cel.pin.get_or_insert_default(), "Pin");
                render_single_value_input(ui, cel.lean_boost_ego.get_or_insert_default(), "Max EGO-in-boost");
                render_single_value_input(ui, cel.lean_boost_map_enable.get_or_insert_default(), "EGO-in-boost MAP threshold (kpa)");
            });

            egui::CollapsingHeader::new("Boost Control").default_open(true).show(ui, |ui| {
                let boost = config.boost_control.get_or_insert_default();
                render_single_value_input(ui, boost.pin.get_or_insert_default(), "Pin");
                render_single_value_input(ui, boost.control_threshold_map.get_or_insert_default(), "Lower MAP threshold (kpa)");
                render_single_value_input(ui, boost.control_threshold_tps.get_or_insert_default(), "Lower TPS threshold (%)");
                render_single_value_input(ui, boost.enable_threshold_map.get_or_insert_default(), "Enable MAP threshold (kpa)");
                render_single_value_input(ui, boost.overboost_map.get_or_insert_default(), "Overboost limit (kpa)");
                egui::CollapsingHeader::new("PWM vs RPM").default_open(false).show(ui, |ui| {
                    Table1dEditor::new().show(ui, boost.pwm_vs_rpm.get_or_insert_default());

                });
            });

        });
    });
}
