use openapiv3::Parameter;
fn foo(p: &Parameter) {
    match p {
        Parameter::Query { parameter_data, .. } => (),
        Parameter::Path { parameter_data, .. } => (),
        Parameter::Header { parameter_data, .. } => (),
        Parameter::Cookie { parameter_data, .. } => (),
    }
}
