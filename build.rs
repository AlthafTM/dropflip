use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=assets/dropflip.rc");
    println!("cargo:rerun-if-changed=assets/dropflip.ico");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let gnu = env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("gnu");
    let resource = output.join(if gnu {
        "dropflip-res.o"
    } else {
        "dropflip.res"
    });
    let status = if gnu {
        Command::new("windres")
            .current_dir("assets")
            .args([
                "--input",
                "dropflip.rc",
                "--output-format",
                "coff",
                "--output",
            ])
            .arg(&resource)
            .status()
    } else {
        Command::new("rc.exe")
            .current_dir("assets")
            .arg("/nologo")
            .arg(format!("/fo{}", resource.display()))
            .arg("dropflip.rc")
            .status()
    }
    .expect("Windows resource compiler is required to embed the app icon");
    assert!(
        status.success(),
        "Could not compile Dropflip's Windows resources"
    );
    println!("cargo:rustc-link-arg={}", resource.display());
}
