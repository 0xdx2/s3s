mod rust;
mod smithy;
mod utils;

mod access;
mod dto;
mod error;
mod headers;
mod minio;
mod ops;
mod s3_trait;
mod sts;
mod xml;

mod aws_conv;
mod aws_proxy;

use std::fs::File;
use std::io::BufWriter;

pub use self::utils::o;

fn write_file(path: &str, f: impl FnOnce()) {
    let mut writer = BufWriter::new(File::create(path).unwrap());
    scoped_writer::scoped(&mut writer, f);
}

pub fn run() {
    let model = {
        let mut s3_model = smithy::Model::load_json("data/s3.json").unwrap();
        let mut sts_model = smithy::Model::load_json("data/sts.json").unwrap();
        sts::reduce(&mut sts_model);
        s3_model.shapes.append(&mut sts_model.shapes);
        minio::patch(&mut s3_model);
        s3_model
    };

    let ops = ops::collect_operations(&model);
    let rust_types = dto::collect_rust_types(&model, &ops);

    {
        let path = "crates/s3s/src/dto/generated.rs";
        write_file(path, || dto::codegen(&rust_types, &ops));
    }

    {
        let path = "crates/s3s/src/header/generated.rs";
        write_file(path, || headers::codegen(&model));
    }

    {
        let path = "crates/s3s/src/error/generated.rs";
        write_file(path, || error::codegen(&model));
    }

    {
        let path = "crates/s3s/src/xml/generated.rs";
        write_file(path, || xml::codegen(&ops, &rust_types));
    }

    {
        let path = "crates/s3s/src/s3_trait.rs";
        write_file(path, || s3_trait::codegen(&ops));
    }

    {
        let path = "crates/s3s/src/ops/generated.rs";
        write_file(path, || ops::codegen(&ops, &rust_types));
    }

    {
        let path = "crates/s3s/src/access/generated.rs";
        write_file(path, || access::codegen(&ops));
    }

    {
        let path = "crates/s3s-aws/src/conv/generated.rs";
        write_file(path, || aws_conv::codegen(&ops, &rust_types));
    }

    {
        let path = "crates/s3s-aws/src/proxy/generated.rs";
        write_file(path, || aws_proxy::codegen(&ops, &rust_types));
    }
}
