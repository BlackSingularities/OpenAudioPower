//! Prints the inputs and outputs of an ONNX graph: cargo run --example inspect -- <model.onnx>
fn main() {
    let path = std::env::args().nth(1).expect("model path");
    let s = ort::session::Session::builder().unwrap().commit_from_file(&path).unwrap();
    for i in &s.inputs {
        println!("input  {:<12} {:?}", i.name, i.input_type);
    }
    for o in &s.outputs {
        println!("output {:<12} {:?}", o.name, o.output_type);
    }
}
