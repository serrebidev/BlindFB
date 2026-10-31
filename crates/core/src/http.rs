//! One shared HTTP agent. TLS verification uses the platform verifier (the
//! operating system's certificate store), not only Mozilla's bundled
//! roots, so downloads also work on networks whose TLS is intercepted by a
//! proxy whose CA the machine itself trusts.

use std::{sync::OnceLock, time::Duration};

pub(crate) fn download_body(body: &mut ureq::Body) -> Result<Vec<u8>, ureq::Error> {
    body.with_config().limit(512 * 1024 * 1024).read_to_vec()
}

pub fn agent() -> ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT
        .get_or_init(|| {
            let tls = ureq::tls::TlsConfig::builder()
                .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                .build();
            ureq::Agent::config_builder()
                .tls_config(tls)
                .timeout_connect(Some(Duration::from_secs(15)))
                .timeout_per_read(Some(Duration::from_secs(30)))
                .timeout_per_write(Some(Duration::from_secs(30)))
                .timeout_global(Some(Duration::from_secs(15 * 60)))
                .build()
                .into()
        })
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downloads_installers_larger_than_ten_megabytes() {
        let bytes = vec![42; 11 * 1024 * 1024];
        let mut body = ureq::Body::builder().data(bytes.clone());
        assert_eq!(download_body(&mut body).unwrap(), bytes);
    }

    #[test]
    fn connections_are_shared_and_network_waits_are_bounded() {
        let first = agent();
        let second = agent();
        assert!(std::ptr::eq(first.config(), second.config()));
        assert_eq!(
            first.config().timeout_per_read(),
            Some(Duration::from_secs(30))
        );
        assert_eq!(
            first.config().timeouts().connect,
            Some(Duration::from_secs(15))
        );
    }
}
