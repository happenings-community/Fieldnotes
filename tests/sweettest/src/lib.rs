//! Conductor setup helpers for Fieldnotes sweettest.
//! Adapted from R&O's tests/sweettest/src/common/conductors.rs, reduced to the
//! three-agent spin-up the cohort-crypto test needs (no progenitor/status logic).

pub mod common {
    use holochain::sweettest::*;
    use holochain::prelude::{AgentPubKey, DnaModifiersOpt, YamlProperties};

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

    /// Path C cross-peer setup: TWO agents on ONE network whose DNA carries a
    /// progenitor property = Alice's pubkey. Alice is the progenitor (eligible
    /// admin); Bob is an ordinary member. Used to prove the integrity zome
    /// gates admin grants by progenitor across real, separate agents, and that
    /// data created by one syncs to the other.
    ///
    /// Returns `(conductors, alice, bob, alice_pubkey)`.
    pub async fn setup_two_agents_with_progenitor(
    ) -> (SweetConductorBatch, SweetCell, SweetCell, AgentPubKey) {
        let mut conductors =
            SweetConductorBatch::from_config_rendezvous(2, SweetConductorConfig::standard())
                .await;

        // Mint Alice's + Bob's keys up front so Alice's key can be burned into
        // the DNA properties as the progenitor BEFORE install.
        let alice_key = SweetAgents::one(conductors[0].keystore()).await;
        let bob_key = SweetAgents::one(conductors[1].keystore()).await;

        // Progenitor property = Alice's pubkey in the 'u'-multibase form the
        // integrity zome's parse_progenitor_pubkey expects (matches to_string()).
        let progenitor_str = alice_key.to_string();
        let mut props = serde_yaml::Mapping::new();
        props.insert(
            serde_yaml::Value::String("progenitor_pubkey".to_string()),
            serde_yaml::Value::String(progenitor_str),
        );
        let modifiers = DnaModifiersOpt::default()
            .with_network_seed("fieldnotes-sweettest".to_string())
            .with_properties(YamlProperties::new(serde_yaml::Value::Mapping(props)));

        let dna = SweetDnaFile::from_bundle_with_overrides(
            std::path::Path::new(DNA_PATH),
            modifiers,
        )
        .await
        .unwrap_or_else(|e| {
            panic!(
                "Failed to load Fieldnotes DNA with progenitor override at {DNA_PATH}: {e}\n\
                 Did you run `cd dna/v1.3 && bash build.sh`?"
            )
        });

        let dna_with_role = ("proofpoll".to_string(), dna.clone());

        let _alice_app = conductors[0]
            .setup_app_for_agent("proofpoll", alice_key.clone(), &[dna_with_role.clone()])
            .await
            .expect("install for Alice failed");
        let _bob_app = conductors[1]
            .setup_app_for_agent("proofpoll", bob_key.clone(), &[dna_with_role.clone()])
            .await
            .expect("install for Bob failed");

        conductors.exchange_peer_info().await;

        let alice = _alice_app.cells()[0].clone();
        let bob = _bob_app.cells()[0].clone();
        (conductors, alice, bob, alice_key)
    }
}
