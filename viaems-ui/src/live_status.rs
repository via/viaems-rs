
use crate::interface;

pub fn render_status_pane(ui: &mut egui::Ui, latest_feed: &interface::EngineUpdate) {

                ui.label("Live Data");
                egui::ScrollArea::vertical().show(ui, |ui| {
                    egui::CollapsingHeader::new("Sensors").default_open(true).show(ui, |ui| {
                        let sensors = latest_feed.sensors.unwrap_or_default();
                        let render_fault = |ui: &mut egui::Ui, f: interface::SensorFault| {
                            match f {
                                interface::SensorFault::SensorNoFault =>
                                    ui.label(egui::RichText::new("OK").color(egui::Color32::GREEN)),
                                interface::SensorFault::SensorRangeFault =>
                                    ui.label(egui::RichText::new("BAD RANGE").color(egui::Color32::RED)),
                                interface::SensorFault::SensorConnectionFault =>
                                    ui.label(egui::RichText::new("BAD CONN").color(egui::Color32::RED)),
                            }
                        };

                        egui::Grid::new("sensors")
                            .num_columns(4)
                            .striped(true)
                            .show(ui, |ui| {
                                ui.label("");
                                ui.label("Value");
                                ui.label("Rate");
                                ui.label("Fault");
                                ui.end_row();

                                ui.label("MAP");
                                ui.label(format!("{:.1}", sensors.map));
                                ui.label(format!("{:.1}", sensors.map_rate));
                                render_fault(ui, sensors.map_fault());
                                ui.end_row();

                                ui.label("IAT");
                                ui.label(format!("{:.1}", sensors.iat));
                                ui.label(format!("{:.1}", sensors.iat_rate));
                                render_fault(ui, sensors.iat_fault());
                                ui.end_row();

                                ui.label("CLT");
                                ui.label(format!("{:.1}", sensors.clt));
                                ui.label(format!("{:.1}", sensors.clt_rate));
                                render_fault(ui, sensors.clt_fault());
                                ui.end_row();

                                ui.label("BRV");
                                ui.label(format!("{:.1}", sensors.brv));
                                ui.label(format!("{:.1}", sensors.brv_rate));
                                render_fault(ui, sensors.brv_fault());
                                ui.end_row();

                                ui.label("TPS");
                                ui.label(format!("{:.1}", sensors.tps));
                                ui.label(format!("{:.1}", sensors.tps_rate));
                                render_fault(ui, sensors.tps_fault());
                                ui.end_row();

                                ui.label("AAP");
                                ui.label(format!("{:.1}", sensors.aap));
                                ui.label(format!("{:.1}", sensors.aap_rate));
                                render_fault(ui, sensors.aap_fault());
                                ui.end_row();

                                ui.label("FRT");
                                ui.label(format!("{:.1}", sensors.frt));
                                ui.label(format!("{:.1}", sensors.frt_rate));
                                render_fault(ui, sensors.frt_fault());
                                ui.end_row();

                                ui.label("EGO");
                                ui.label(format!("{:.1}", sensors.ego));
                                ui.label(format!("{:.1}", sensors.ego_rate));
                                render_fault(ui, sensors.ego_fault());
                                ui.end_row();

                                ui.label("FRP");
                                ui.label(format!("{:.1}", sensors.frp));
                                ui.label(format!("{:.1}", sensors.frp_rate));
                                render_fault(ui, sensors.frp_fault());
                                ui.end_row();

                                ui.label("ETH");
                                ui.label(format!("{:.1}", sensors.eth));
                                ui.label(format!("{:.1}", sensors.eth_rate));
                                render_fault(ui, sensors.eth_fault());
                                ui.end_row();

                                ui.label("KNK1");
                                ui.label(sensors.knock1.to_string());
                                ui.end_row();

                                ui.label("KNK2");
                                ui.label(sensors.knock2.to_string());
                                ui.end_row();

                            });

                    });
                    egui::CollapsingHeader::new("Position").default_open(true).show(ui, |ui| {
                        let position = latest_feed.position.unwrap_or_default();
                        egui::Grid::new("position")
                            .num_columns(2)
                            .striped(true)
                            .show(ui, |ui| {
                                ui.label("Sync");
                                if position.synced {
                                    ui.label(egui::RichText::new("YES").color(egui::Color32::GREEN));
                                } else {
                                    ui.label(egui::RichText::new("NO").color(egui::Color32::RED));
                                }
                                ui.end_row();

                                if !position.synced {
                                    ui.label("Reason");
                                    let reason = position.loss_cause().as_str_name();
                                    let stripped = if let Some(s) = reason.strip_prefix("DECODER_") { s } else { reason };

                                    ui.label(stripped);
                                    ui.end_row();
                                }

                                ui.label("Angle");
                                ui.label(position.last_angle.to_string());
                                ui.end_row();

                                ui.label("RPM");
                                ui.label(position.average_rpm.to_string());
                                ui.end_row();

                            });
                        });
                    egui::CollapsingHeader::new("Calculations").default_open(true).show(ui, |ui| {
                        let calcs = latest_feed.calculations.unwrap_or_default();
                        egui::Grid::new("calcs")
                            .num_columns(2)
                            .striped(true)
                            .show(ui, |ui| {
                                ui.label("Timing Advance");
                                ui.label(format!("{:.0}", calcs.advance));
                                ui.end_row();

                                ui.label("Dwell (uS)");
                                ui.label(format!("{:.0}", calcs.dwell_us));
                                ui.end_row();

                                ui.label("Fuel (uS)");
                                ui.label(format!("{:.0}", calcs.fuel_us));
                                ui.end_row();

                                ui.label("Lambda");
                                ui.label(format!("{:.3}", calcs.lambda));
                                ui.end_row();

                                ui.label("VE");
                                ui.label(format!("{:.1}", calcs.ve));
                                ui.end_row();

                                ui.label("Temp Enrichment (%)");
                                ui.label(format!("{:.0}", calcs.engine_temp_enrichment));
                                ui.end_row();

                                ui.separator();
                                ui.end_row();

                                ui.label("Airmass per cycle (g)");
                                ui.label(format!("{:.3}", calcs.airmass_per_cycle));
                                ui.end_row();

                                ui.label("Fuel volume per cycle (cc)");
                                ui.label(format!("{:.3}", calcs.fuelvol_per_cycle));
                                ui.end_row();

                                ui.label("Pulsewidth Correction (mS)");
                                ui.label(format!("{:.3}", calcs.pulse_width_correction));
                                ui.end_row();

                                ui.separator();
                                ui.end_row();

                                ui.label("RPM Limiter");
                                if calcs.rpm_limit_cut {
                                    ui.label(egui::RichText::new("ON").color(egui::Color32::RED));
                                } else {
                                    ui.label(egui::RichText::new("OFF").color(egui::Color32::GREEN));
                                }
                                ui.end_row();

                                ui.label("Boost Limiter");
                                if calcs.boost_cut {
                                    ui.label(egui::RichText::new("ON").color(egui::Color32::RED));
                                } else {
                                    ui.label(egui::RichText::new("OFF").color(egui::Color32::GREEN));
                                }
                                ui.end_row();

                                ui.label("Fuel Limiter");
                                if calcs.fuel_overduty_cut {
                                    ui.label(egui::RichText::new("ON").color(egui::Color32::RED));
                                } else {
                                    ui.label(egui::RichText::new("OFF").color(egui::Color32::GREEN));
                                }
                                ui.end_row();

                                ui.label("Dwell Limiter");
                                if calcs.dwell_overduty_cut {
                                    ui.label(egui::RichText::new("ON").color(egui::Color32::RED));
                                } else {
                                    ui.label(egui::RichText::new("OFF").color(egui::Color32::GREEN));
                                }
                                ui.end_row();

                            });
                        });
                    egui::Grid::new("feed")
                        .num_columns(2)
                        .striped(true)
                        .show(ui, |ui| {
                            ui.label("cputime");
                            ui.label(latest_feed.header.unwrap_or_default().timestamp.to_string());
                            ui.end_row();
                        });
                });


}
