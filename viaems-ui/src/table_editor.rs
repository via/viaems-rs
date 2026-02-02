use crate::interface::configuration;

pub struct Table2dEditor {
}

impl Table2dEditor {

    pub fn new() -> Self {
        Table2dEditor {
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, table: &mut configuration::Table2d) -> bool {
        let mut modified = false;

        egui::Grid::new("table")
            .num_columns(table.cols.as_ref().unwrap().values.len())
            .striped(true)
            .show(ui, |ui| {
                if let Some(cols) = &table.cols {
                    ui.label("   ");
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
                        let field = egui::DragValue::new(col).update_while_editing(false);
                        if ui.add(field).changed() {
                            println!("Changed {}:{} to {}", row_idx, col_idx, col);
                            modified = true;
                        }

                    }
                    ui.end_row();
                }
            });
        modified
    }

}

