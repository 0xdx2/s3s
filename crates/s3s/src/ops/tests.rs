use crate::host::MultiDomain;

use super::*;

// use crate::service::S3Service;

// use stdx::mem::output_size;

// #[test]
// #[ignore]
// fn track_future_size() {
//     macro_rules! future_size {
//         ($f:path, $v:expr) => {
//             (stringify!($f), output_size(&$f), $v)
//         };
//     }

//     #[rustfmt::skip]
//     let sizes = [
//         future_size!(S3Service::call,                           2704),
//         future_size!(call,                                      1512),
//         future_size!(prepare,                                   1440),
//         future_size!(SignatureContext::check,                   776),
//         future_size!(SignatureContext::v2_check,                296),
//         future_size!(SignatureContext::v2_check_presigned_url,  168),
//         future_size!(SignatureContext::v2_check_header_auth,    184),
//         future_size!(SignatureContext::v4_check,                752),
//         future_size!(SignatureContext::v4_check_post_signature, 368),
//         future_size!(SignatureContext::v4_check_presigned_url,  456),
//         future_size!(SignatureContext::v4_check_header_auth,    640),
//     ];

//     println!("{sizes:#?}");
//     for (name, size, expected) in sizes {
//         assert_eq!(size, expected, "{name:?} size changed: prev {expected}, now {size}");
//     }
// }

#[test]
fn error_custom_headers() {
    fn redirect307(location: &str) -> S3Error {
        let mut err = S3Error::new(S3ErrorCode::TemporaryRedirect);

        err.set_headers({
            let mut headers = HeaderMap::new();
            headers.insert(crate::header::LOCATION, location.parse().unwrap());
            headers
        });

        err
    }

    let res = serialize_error(redirect307("http://example.com"), false).unwrap();
    assert_eq!(res.status, StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(res.headers.get("location").unwrap(), "http://example.com");

    let body = res.body.bytes().unwrap();
    let body = std::str::from_utf8(&body).unwrap();
    assert_eq!(
        body,
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
            "<Error><Code>TemporaryRedirect</Code></Error>"
        )
    );
}

#[test]
fn extract_host_from_uri() {
    use crate::http::Request;
    use crate::ops::extract_host;

    let mut req = Request::from(
        hyper::Request::builder()
            .method(Method::GET)
            .version(::http::Version::HTTP_2)
            .uri("https://test.example.com:9001/rust.pdf?X-Amz-Algorithm=AWS4-HMAC-SHA256&X-Amz-Date=20251213T084305Z&X-Amz-SignedHeaders=host&X-Amz-Credential=rustfsadmin%2F20251213%2Fus-east-1%2Fs3%2Faws4_request&X-Amz-Expires=3600&X-Amz-Signature=57133ee54dab71c00a10106c33cde2615b301bd2cf00e2439f3ddb4bc999ec66")
            .body(Body::empty())
            .unwrap(),
    );

    let host = extract_host(&req).unwrap();
    assert_eq!(host, Some("test.example.com:9001".to_string()));

    req.version = ::http::Version::HTTP_11;
    let host = extract_host(&req).unwrap();
    assert_eq!(host, None);

    req.version = ::http::Version::HTTP_3;
    let host = extract_host(&req).unwrap();
    assert_eq!(host, Some("test.example.com:9001".to_string()));

    let mut req = Request::from(
        hyper::Request::builder()
            .version(::http::Version::HTTP_10)
            .method(Method::GET)
            .uri("http://another.example.org/resource")
            .body(Body::empty())
            .unwrap(),
    );
    let host = extract_host(&req).unwrap();
    assert_eq!(host, None);

    req.version = ::http::Version::HTTP_2;
    let host = extract_host(&req).unwrap();
    assert_eq!(host, Some("another.example.org".to_string()));

    req.version = ::http::Version::HTTP_3;
    let host = extract_host(&req).unwrap();
    assert_eq!(host, Some("another.example.org".to_string()));

    let req = Request::from(
        hyper::Request::builder()
            .method(Method::GET)
            .uri("/no/host/header")
            .header("Host", "header.example.com:8080")
            .body(Body::empty())
            .unwrap(),
    );
    let host = extract_host(&req).unwrap();
    assert_eq!(host, Some("header.example.com:8080".to_string()));

    let req = Request::from(
        hyper::Request::builder()
            .method(Method::GET)
            .uri("/no/host/header")
            .body(Body::empty())
            .unwrap(),
    );
    let host = extract_host(&req).unwrap();
    assert_eq!(host, None);
}


#[test]
fn vh_no_bucket_2_root() {
    let domains = ["example.com:9000", "example.com:9001", "example.io", "example.com", "example.io:9000", "example.io:9001"];
    let md = MultiDomain::new(domains.iter().copied()).unwrap();

    let host = "example.com:9000";
    let result = md.parse_host_header(host);
    let vh = result.unwrap();
    assert_eq!(vh.domain(), host);
    assert_eq!(vh.bucket(), None);
}

/// Tests for virtual-hosted-style request parsing with fallback to path-style
mod virtual_host_parsing_logic_tests {
    use super::*;
    use crate::host::MultiDomain;
    use crate::path::{parse_path_style, parse_virtual_hosted_style, S3Path};

    /// Test: bucket.example.com/object.txt
    #[test]
    fn test_vh_with_bucket_and_key() {
        let domains = ["example.com:9000", "example.com:9001", "example.io", "example.com", "example.io:9000", "example.io:9001"];
        let s3_host = MultiDomain::new(domains.iter().copied()).unwrap();
        let host_header = "mybucket.example.com";
        let uri_path = "/object.txt";

        let vh = s3_host.parse_host_header(host_header).unwrap();
        let vh_bucket = vh.bucket();
        
        assert_eq!(vh_bucket, Some("mybucket"));
        
        let result = parse_virtual_hosted_style(vh_bucket, uri_path).unwrap();
        match result {
            S3Path::Object { bucket, key } => {
                assert_eq!(bucket.as_ref(), "mybucket");
                assert_eq!(key.as_ref(), "object.txt");
            }
            _ => panic!("Expected S3Path::Object, got {:?}", result),
        }
    }

    /// Test: bucket.example.com/ (bucket only, no key)
    #[test]
    fn test_vh_with_bucket_only() {
        let domains = ["example.com:9000", "example.com:9001", "example.io", "example.com", "example.io:9000", "example.io:9001"];
        let s3_host = MultiDomain::new(domains.iter().copied()).unwrap();
        let host_header = "mybucket.example.com";
        let uri_path = "/";

        let vh = s3_host.parse_host_header(host_header).unwrap();
        let vh_bucket = vh.bucket();
        
        assert_eq!(vh_bucket, Some("mybucket"));
        
        let result = parse_virtual_hosted_style(vh_bucket, uri_path).unwrap();
        match result {
            S3Path::Bucket { bucket } => {
                assert_eq!(bucket.as_ref(), "mybucket");
            }
            _ => panic!("Expected S3Path::Bucket, got {:?}", result),
        }
    }

    /// Test: example.com/mybucket/myfile.txt (fallback to path-style)
    #[test]
    fn test_vh_no_bucket_fallback_to_path_style_with_bucket_and_key() {
        let domains = ["example.com:9000", "example.com:9001", "example.io", "example.com", "example.io:9000", "example.io:9001"];
        let s3_host = MultiDomain::new(domains.iter().copied()).unwrap();
        let host_header = "example.com";
        let uri_path = "/mybucket/myfile.txt";

        let vh = s3_host.parse_host_header(host_header).unwrap();
        let vh_bucket = vh.bucket();
        
        assert_eq!(vh_bucket, None);
        
        let result = parse_path_style(uri_path).unwrap();
        match result {
            S3Path::Object { bucket, key } => {
                assert_eq!(bucket.as_ref(), "mybucket");
                assert_eq!(key.as_ref(), "myfile.txt");
            }
            _ => panic!("Expected S3Path::Object, got {:?}", result),
        }
    }

    /// Test: example.com/mybucket/ (fallback to path-style, bucket only)
    #[test]
    fn test_vh_no_bucket_fallback_to_path_style_bucket_only() {
        let domains = ["example.com:9000", "example.com:9001", "example.io", "example.com", "example.io:9000", "example.io:9001"];
        let s3_host = MultiDomain::new(domains.iter().copied()).unwrap();
        let host_header = "example.com";
        let uri_path = "/mybucket/";

        let vh = s3_host.parse_host_header(host_header).unwrap();
        let vh_bucket = vh.bucket();
        
        assert_eq!(vh_bucket, None);
        
        let result = parse_path_style(uri_path).unwrap();
        match result {
            S3Path::Bucket { bucket } => {
                assert_eq!(bucket.as_ref(), "mybucket");
            }
            _ => panic!("Expected S3Path::Bucket, got {:?}", result),
        }
    }

    /// Test: example.com/ and example.com/favicon.ico (fallback to path-style)
    #[test]
    fn test_vh_no_bucket_fallback_to_path_style_root() {
        let domains = ["example.com:9000", "example.com:9001", "example.io", "example.com", "example.io:9000", "example.io:9001"];
        let s3_host = MultiDomain::new(domains.iter().copied()).unwrap();
        let host_header = "example.com";

        let vh = s3_host.parse_host_header(host_header).unwrap();
        let vh_bucket = vh.bucket();
        
        assert_eq!(vh_bucket, None);
        
        let result = parse_path_style("/").unwrap();
        match result {
            S3Path::Root => {}
            _ => panic!("Expected S3Path::Root, got {:?}", result),
        }

        let result_single = parse_path_style("/favicon.ico").unwrap();
        match result_single {
            S3Path::Bucket { bucket } => {
                assert_eq!(bucket.as_ref(), "favicon.ico");
            }
            _ => panic!("Expected S3Path::Bucket for single segment, got {:?}", result_single),
        }
    }

    /// Test: bucket.example.com:9000/object.txt (with port)
    #[test]
    fn test_vh_with_port_and_bucket() {
        let domains = ["example.com:9000", "example.com:9001", "example.io", "example.com", "example.io:9000", "example.io:9001"];
        let s3_host = MultiDomain::new(domains.iter().copied()).unwrap();
        let host_header = "mybucket.example.com:9000";
        let uri_path = "/object.txt";

        let vh = s3_host.parse_host_header(host_header).unwrap();
        let vh_bucket = vh.bucket();
        
        assert_eq!(vh_bucket, Some("mybucket"));
        
        let result = parse_virtual_hosted_style(vh_bucket, uri_path).unwrap();
        match result {
            S3Path::Object { bucket, key } => {
                assert_eq!(bucket.as_ref(), "mybucket");
                assert_eq!(key.as_ref(), "object.txt");
            }
            _ => panic!("Expected S3Path::Object, got {:?}", result),
        }
    }

    /// Test: example.com:9000/mybucket/myfile.txt (with port, fallback to path-style)
    #[test]
    fn test_vh_with_port_no_bucket_fallback() {
        let domains = ["example.com:9000", "example.com:9001", "example.io", "example.com", "example.io:9000", "example.io:9001"];
        let s3_host = MultiDomain::new(domains.iter().copied()).unwrap();
        let host_header = "example.com:9000";
        let uri_path = "/mybucket/myfile.txt";

        let vh = s3_host.parse_host_header(host_header).unwrap();
        let vh_bucket = vh.bucket();
        
        assert_eq!(vh_bucket, None);
        
        let result = parse_path_style(uri_path).unwrap();
        match result {
            S3Path::Object { bucket, key } => {
                assert_eq!(bucket.as_ref(), "mybucket");
                assert_eq!(key.as_ref(), "myfile.txt");
            }
            _ => panic!("Expected S3Path::Object, got {:?}", result),
        }
    }

    /// Test: path-style without virtual host configuration
    #[test]
    fn test_path_style_without_virtual_host() {
        let uri_path = "/mybucket/myfile.txt";

        let result = parse_path_style(uri_path).unwrap();
        match result {
            S3Path::Object { bucket, key } => {
                assert_eq!(bucket.as_ref(), "mybucket");
                assert_eq!(key.as_ref(), "myfile.txt");
            }
            _ => panic!("Expected S3Path::Object, got {:?}", result),
        }
    }

    /// Test: bucket.example.io:9001/object.txt (different domain with port)
    #[test]
    fn test_vh_with_different_domain_and_port() {
        let domains = ["example.com:9000", "example.com:9001", "example.io", "example.com", "example.io:9000", "example.io:9001"];
        let s3_host = MultiDomain::new(domains.iter().copied()).unwrap();
        let host_header = "mybucket.example.io:9001";
        let uri_path = "/object.txt";

        let vh = s3_host.parse_host_header(host_header).unwrap();
        let vh_bucket = vh.bucket();
        
        assert_eq!(vh_bucket, Some("mybucket"));
        
        let result = parse_virtual_hosted_style(vh_bucket, uri_path).unwrap();
        match result {
            S3Path::Object { bucket, key } => {
                assert_eq!(bucket.as_ref(), "mybucket");
                assert_eq!(key.as_ref(), "object.txt");
            }
            _ => panic!("Expected S3Path::Object, got {:?}", result),
        }
    }

    /// Test: example.io:9000/mybucket/file.txt (different domain with port, fallback)
    #[test]
    fn test_vh_different_domain_port_no_bucket_fallback() {
        let domains = ["example.com:9000", "example.com:9001", "example.io", "example.com", "example.io:9000", "example.io:9001"];
        let s3_host = MultiDomain::new(domains.iter().copied()).unwrap();
        let host_header = "example.io:9000";
        let uri_path = "/mybucket/myfile.txt";

        let vh = s3_host.parse_host_header(host_header).unwrap();
        let vh_bucket = vh.bucket();
        
        assert_eq!(vh_bucket, None);
        
        let result = parse_path_style(uri_path).unwrap();
        match result {
            S3Path::Object { bucket, key } => {
                assert_eq!(bucket.as_ref(), "mybucket");
                assert_eq!(key.as_ref(), "myfile.txt");
            }
            _ => panic!("Expected S3Path::Object, got {:?}", result),
        }
    }
}
