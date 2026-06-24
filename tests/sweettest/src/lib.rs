//! Conductor setup helpers for Fieldnotes sweettest.
//! Adapted from R&O's tests/sweettest/src/common/conductors.rs, reduced to the
//! three-agent spin-up the cohort-crypto test needs (no progenitor/status logic).

pub mod common {
    use holochain::sweettest::*;

    /// Path to the compiled Fieldnotes DNA bundle, relative to this crate.
    pub const DNA_PATH: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../dna/v1.3/workdir/proofpoll_v1_3.dna"
    );

    /// Spin up three conductors, each with the Fieldnotes DNA installed under
    /// the `proofpoll` app id. Dev-mode properties (no progenitor) are fine —
    /// the crypto functions don't touch the admin gate.
    ///
    /// Returns `(conductors, alice, bob, carol)`.
    pub async fn setup_three_agents(
    ) -> (SweetConductorBatch, SweetCell, SweetCell, SweetCell) {
        let mut conductors =
            SweetConductorBatch::from_config_rendezvous(3, SweetConductorConfig::standard())
                .await;

        let dna = SweetDnaFile::from_bundle(std::path::Path::new(DNA_PATH))
            .await
            .unwrap_or_else(|e| {
                panic!(
                    "Failed to load Fieldnotes DNA bundle at {DNA_PATH}: {e}\n\
                     Did you run `cd dna/v1.3 && bash build.sh`?"
                )
            });

        let apps = conductors
            .setup_app("proofpoll", &[dna])
            .await
            .expect("Failed to install Fieldnotes app");

        conductors.exchange_peer_info().await;

        let ((alice,), (bob,), (carol,)) = apps.into_tuples();
        (conductors, alice, bob, carol)
    }
}
