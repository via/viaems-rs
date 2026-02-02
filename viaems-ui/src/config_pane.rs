use crate::interface;
use crate::table_editor::Table2dEditor;


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

        ui.horizontal(|ui| {
            ui.label("Pin");
            let mut pintext = sensor.pin().to_string();
            ui.add(egui::TextEdit::singleline(&mut pintext).desired_width(40.0));
            sensor.pin = pintext.parse().ok();
        });

        ui.horizontal(|ui| {
            ui.label("Lag filter");
            let mut lagtext = sensor.lag().to_string();
            ui.add(egui::TextEdit::singleline(&mut lagtext).desired_width(40.0));
            sensor.lag = lagtext.parse().ok();
        });

        ui.separator();

        if sourcevalue == interface::configuration::SensorSource::SourceConst {
            let mut cc = sensor.const_config.unwrap_or_default();
            ui.horizontal(|ui| {
                ui.label("Fixed Value)");
                let mut val = cc.fixed_value.to_string();
                ui.add(egui::TextEdit::singleline(&mut val).desired_width(40.0));
                cc.fixed_value = val.parse().unwrap_or(0.0);
            });
            sensor.const_config = Some(cc);
        } else {
            if (methodvalue == interface::configuration::SensorMethod::MethodLinear) ||
               (methodvalue == interface::configuration::SensorMethod::MethodLinearWindowed) {
                let mut lc = sensor.linear_config.unwrap_or_default();

                ui.horizontal(|ui| {
                    ui.label("Input Range");
                    let mut input_min = lc.input_min.to_string();
                    ui.add(egui::TextEdit::singleline(&mut input_min).desired_width(40.0));
                    lc.input_min = input_min.parse().unwrap_or(0.0);

                    let mut input_max = lc.input_max.to_string();
                    ui.add(egui::TextEdit::singleline(&mut input_max).desired_width(40.0));
                    lc.input_max = input_max.parse().unwrap_or(5.0);

                });

                ui.horizontal(|ui| {
                    ui.label("Output Range");
                    let mut out_min = lc.output_min.to_string();
                    ui.add(egui::TextEdit::singleline(&mut out_min).desired_width(40.0));
                    lc.output_min = out_min.parse().unwrap_or(0.0);

                    let mut out_max = lc.output_max.to_string();
                    ui.add(egui::TextEdit::singleline(&mut out_max).desired_width(40.0));
                    lc.output_max = out_max.parse().unwrap_or(100.0);
                });

                sensor.linear_config = Some(lc);

                if methodvalue == interface::configuration::SensorMethod::MethodLinearWindowed {
                    ui.separator();
                    let mut wc = sensor.window_config.unwrap_or_default();

                    ui.horizontal(|ui| {
                        ui.label("Window Capture Opening");
                        let mut capture = wc.capture_width.to_string();
                        ui.add(egui::TextEdit::singleline(&mut capture).desired_width(40.0));
                        wc.capture_width = capture.parse().unwrap_or(0.0);
                    });

                    ui.horizontal(|ui| {
                        ui.label("Window Total Width");
                        let mut total = wc.total_width.to_string();
                        ui.add(egui::TextEdit::singleline(&mut total).desired_width(40.0));
                        wc.total_width = total.parse().unwrap_or(0.0);
                    });

                    ui.horizontal(|ui| {
                        ui.label("Window offset");
                        let mut offset = wc.offset.to_string();
                        ui.add(egui::TextEdit::singleline(&mut offset).desired_width(40.0));
                        wc.offset = offset.parse().unwrap_or(0.0);
                    });

                    sensor.window_config = Some(wc);

                }
        } else if methodvalue == interface::configuration::SensorMethod::MethodThermistor {
            let mut tc = sensor.thermistor_config.unwrap_or_default();

            ui.horizontal(|ui| {
                ui.label("Bias (Ohms)");
                let mut bias = tc.bias.to_string();
                ui.add(egui::TextEdit::singleline(&mut bias).desired_width(40.0));
                tc.bias = bias.parse().unwrap_or(0.0);
            });

            ui.horizontal(|ui| {
                ui.label("A");
                let mut A = tc.a.to_string();
                ui.add(egui::TextEdit::singleline(&mut A).desired_width(80.0));
                tc.a = A.parse().unwrap_or(0.0);
            });

            ui.horizontal(|ui| {
                ui.label("B");
                let mut B = tc.b.to_string();
                ui.add(egui::TextEdit::singleline(&mut B).desired_width(80.0));
                tc.b = B.parse().unwrap_or(0.0);
            });

            ui.horizontal(|ui| {
                ui.label("C");
                let mut C = tc.c.to_string();
                ui.add(egui::TextEdit::singleline(&mut C).desired_width(80.0));
                tc.c = C.parse().unwrap_or(0.0);
            });


            sensor.thermistor_config = Some(tc);
        }
            let mut fc = sensor.fault_config.unwrap_or_default();

            ui.separator();

            ui.horizontal(|ui| {
                ui.label("Minimum Input");
                let mut minimum = fc.min.to_string();
                ui.add(egui::TextEdit::singleline(&mut minimum).desired_width(40.0));
                fc.min = minimum.parse().unwrap_or(0.0);
            });

            ui.horizontal(|ui| {
                ui.label("Maximum Input");
                let mut maximum = fc.max.to_string();
                ui.add(egui::TextEdit::singleline(&mut maximum).desired_width(40.0));
                fc.max = maximum.parse().unwrap_or(5.0);
            });

            ui.horizontal(|ui| {
                ui.label("Fallback value");
                let mut val = fc.value.to_string();
                ui.add(egui::TextEdit::singleline(&mut val).desired_width(40.0));
                fc.value = val.parse().unwrap_or(0.0);
            });

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

                        let mut pin = output.pin().to_string();
                        ui.add(egui::TextEdit::singleline(&mut pin).desired_width(40.0));
                        output.pin = pin.parse().ok();

                        let mut angle = output.angle().to_string();
                        ui.add(egui::TextEdit::singleline(&mut angle).desired_width(40.0));
                        output.angle = angle.parse().ok();

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
            egui::CollapsingHeader::new("Timing").show(ui, |ui| {
              if let Some(ignition) = &mut config.ignition &&
                 let Some(timing) = &mut ignition.timing {
                    Table2dEditor::new().show(ui, timing);
              }
            });
        });

        egui::CollapsingHeader::new("Fueling").default_open(true).show(ui, |ui| {
            egui::CollapsingHeader::new("VE").show(ui, |ui| {
              if let Some(fueling) = &mut config.fueling &&
                 let Some(ve) = &mut fueling.ve {
                    Table2dEditor::new().show(ui, ve);
              }
            });
            egui::CollapsingHeader::new("Lambda").show(ui, |ui| {
              if let Some(fueling) = &mut config.fueling &&
                 let Some(lambda) = &mut fueling.commanded_lambda {
                    Table2dEditor::new().show(ui, lambda);
              }
            });
        });

        egui::CollapsingHeader::new("Decoder").default_open(true).show(ui, |ui| {
        });

        egui::CollapsingHeader::new("Misc").default_open(true).show(ui, |ui| {
            egui::CollapsingHeader::new("Rpm Cut").default_open(true).show(ui, |ui| {
            });

            egui::CollapsingHeader::new("Check Engine Light").default_open(true).show(ui, |ui| {
            });

            egui::CollapsingHeader::new("Boost Control").default_open(true).show(ui, |ui| {
            });

        });
    });
}
