use crate::interface::configuration;

pub struct Table2dEditor {
}

impl Table2dEditor {

    pub fn new() -> Self {
        Table2dEditor {
        }
    }

    pub fn show(&mut self, 
                ui: &mut egui::Ui, 
                reference: &configuration::Table2d,
                table: &mut configuration::Table2d) -> bool {
        let mut modified = false;

        egui::Grid::new("table")
            .num_columns(table.cols.as_ref().unwrap().values.len() + 1)
            .striped(true)
            .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.menu_button("⚙", |ui| {
                            let _ = ui.button("TODO");
                        });
                        if *reference != *table {
                            if ui.button("↺").clicked() {
                                *table = reference.clone();
                            }
                        }
                    });
                if let Some(cols) = &table.cols {
                    for val in &cols.values {
                        let value = format!("{:3}", val);
                        ui.label(value);
                    }
                    ui.end_row();
                }


                for (row_idx, row) in table.data.iter_mut().enumerate() {
                    let row_label = if let Some(rowaxis) = &table.rows {
                        rowaxis.values[row_idx]
                    } else {
                        0.0
                    };
                    let row_label_str = format!("{:3}", row_label);
                    ui.label(row_label_str);

                    for (col_idx, col) in row.values.iter_mut().enumerate() {
                        let field = egui::DragValue::new(col)
                            .update_while_editing(false);
                        if ui.add(field).changed() {
                            modified = true;
                        }

                    }
                    ui.end_row();
                }
            });
        modified
    }

}


pub struct Table1dEditor {
}

impl Table1dEditor {

    pub fn new() -> Self {
        Table1dEditor {
        }
    }

    pub fn show(&mut self, 
                ui: &mut egui::Ui, 
                reference: &configuration::Table1d,
                table: &mut configuration::Table1d) -> bool {
        let mut modified = false;

        egui::Grid::new("table")
            .num_columns(2)
            .striped(true)
            .show(ui, |ui| {
                ui.label(table.cols.get_or_insert_default().name.clone().unwrap_or("Value".to_string()));
                ui.horizontal(|ui| {
                    if ui.button("⚙").clicked() {
                    }
                    if *reference != *table {
                        if ui.button("↺").clicked() {
                            *table = reference.clone();
                        }
                    }
                });
                ui.end_row();
                let cols = table.data.get_or_insert_default();
                for (col_idx, col) in cols.values.iter_mut().enumerate() {
                    let col_label = if let Some(colaxis) = &table.cols {
                        colaxis.values[col_idx]
                    } else {
                        0.0
                    };
                    let col_label_str = format!("{:3}", col_label);
                    ui.label(col_label_str);

                    let field = egui::DragValue::new(col).update_while_editing(false);
                    if ui.add(field).changed() {
                        modified = true;
                    }
                    ui.end_row();
                }
            });
        modified
    }

}
