use std::io::Result;
fn main() -> Result<()> {
    let mut config = prost_build::Config::new();
    config.type_attribute("viaems.console.EngineUpdate", "#[derive(viaems_interface_derive::LoggableStruct)]");
    config.type_attribute("viaems.console.Header", "#[derive(viaems_interface_derive::LoggableStruct)]");
    config.type_attribute("viaems.console.Sensors", "#[derive(viaems_interface_derive::LoggableStruct)]");
    config.type_attribute("viaems.console.Position", "#[derive(viaems_interface_derive::LoggableStruct)]");
    config.type_attribute("viaems.console.Calculations", "#[derive(viaems_interface_derive::LoggableStruct)]");
    config.compile_protos(&["src/console.proto"], &["src/"])?;
    Ok(())
}
