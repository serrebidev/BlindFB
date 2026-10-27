//! One shared HTTP agent. TLS verification uses the platform verifier (the
//! operating system's certificate store), not only Mozilla's bundled
//! roots, so downloads also work on networks whose TLS is intercepted by a
//! proxy whose CA the machine itself trusts.

pub fn agent() -> ureq::Agent {
    let tls = ureq::tls::TlsConfig::builder()
        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
        .build();
    ureq::Agent::config_builder()
        .tls_config(tls)
        .build()
        .into()
}
