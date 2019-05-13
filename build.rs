use std::fs::{DirBuilder, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

fn main() {
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let out_dir = Path::new(&out_dir).join("protos");

    let in_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("src");
    // Re-run this build.rs if the protos dir changes (i.e. a new file is added)
    println!("cargo:rerun-if-changed={}", in_dir.to_str().unwrap());

    // Find all *.proto files in the `in_dir` and add them to the list of files
    let mut protos = Vec::new();
    let proto_ext = Some(Path::new("proto").as_os_str());
    for entry in WalkDir::new(&in_dir) {
        let path = entry.unwrap().into_path();
        if path.extension() == proto_ext {
            // Re-run this build.rs if any of the files in the protos dir change
            println!("cargo:rerun-if-changed={}", path.to_str().unwrap());
            protos.push(path);
        }
    }

    // Delete all old generated files before re-generating new ones
    if out_dir.exists() {
        std::fs::remove_dir_all(&out_dir).unwrap();
    }
    DirBuilder::new().create(&out_dir).unwrap();

    write_mod_rs(&protos, &in_dir, &out_dir);
    generate_protos(&protos, &in_dir, &out_dir);
}

fn write_mod_rs(protos: &[PathBuf], in_dir: &Path, out_dir: &Path) {
    let mod_rs = protos
        .iter()
        .map(|proto| {
            format!(
                "/// Generated from {:?}\npub mod {};\n",
                proto.strip_prefix(&in_dir).unwrap(),
                proto.file_stem().unwrap().to_str().unwrap()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    File::create(out_dir.join("mod.rs"))
        .unwrap()
        .write_all(mod_rs.as_bytes())
        .unwrap();
}

fn generate_protos(protos: &[PathBuf], in_dir: &Path, out_dir: &Path) {
    let input = protos
        .iter()
        .map(|f| f.to_str().unwrap())
        .collect::<Vec<_>>();
    let includes = &[in_dir.to_str().unwrap()];
    protobuf_codegen_pure::run(protobuf_codegen_pure::Args {
        out_dir: &out_dir.to_str().unwrap(),
        input: &input,
        includes,
        customize: Default::default(),
    })
    .expect("protoc");
}
