fn main() {
    if !fitsend_core::video_tools_available() {
        eprintln!("FitSend could not find working FFmpeg and FFprobe tools.");
        std::process::exit(2);
    }

    let mut arguments = std::env::args().skip(1);
    let Some(input_path) = arguments.next() else {
        println!("FitSend found working FFmpeg and FFprobe tools.");
        return;
    };
    let output_path = arguments
        .next()
        .expect("provide an output path after the input path");
    let target_bytes = 700 * 1024;
    let analysis = fitsend_core::analyze(&input_path).expect("the smoke-test video should analyze");
    let result = fitsend_core::process(&fitsend_core::ProcessRequest {
        analysis,
        target_bytes,
        output_path,
    })
    .expect("the smoke-test video should process");
    assert!(result.verified);
    assert!(result.output_bytes <= target_bytes);
    println!(
        "Bundled video pipeline verified: {} bytes after {} attempt(s).",
        result.output_bytes, result.attempts
    );
}
