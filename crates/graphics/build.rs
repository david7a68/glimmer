use std::path::PathBuf;

use windows::{s, w};

enum ShaderKind {
    Vertex,
    Pixel,
}

fn main() {
    println!("cargo:rerun-if-changed=shaders");

    compile_shaders();
}

fn compile_shaders() {
    compile(
        w!("shaders/ui.hlsl"),
        ShaderKind::Vertex,
        s!("vertex_main"),
        "ui_vs.cso",
    );
    compile(
        w!("shaders/ui.hlsl"),
        ShaderKind::Pixel,
        s!("pixel_main"),
        "ui_ps.cso",
    );
}

fn compile(
    path: windows::core::PCWSTR,
    kind: ShaderKind,
    entrypoint: windows::core::PCSTR,
    artifact_name: &str,
) {
    use windows::Win32::Graphics::Direct3D::Fxc::*;

    let mut code = None;
    let mut errors = None;

    let shader = match kind {
        ShaderKind::Vertex => s!("vs_5_1"),
        ShaderKind::Pixel => s!("ps_5_1"),
    };

    let _ = unsafe {
        D3DCompileFromFile(
            path,
            None,
            None,
            entrypoint,
            shader,
            0,
            0,
            &mut code,
            Some(&mut errors),
        )
    };

    if let Some(errors) = errors {
        let eptr = unsafe { errors.GetBufferPointer() };
        let estr = unsafe { std::slice::from_raw_parts(eptr.cast(), errors.GetBufferSize()) };
        let errors = String::from_utf8_lossy(estr);
        panic!("{}", errors);
    }

    let code = code.unwrap();
    let bytes: &[u8] =
        unsafe { std::slice::from_raw_parts(code.GetBufferPointer().cast(), code.GetBufferSize()) };

    let mut out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    out.push(artifact_name);
    std::fs::write(out, bytes).unwrap();
}
