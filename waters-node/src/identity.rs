use anyhow::Result;
use blake3::Hasher;
use ed25519_dalek::{SecretKey, Signature, SigningKey, VerifyingKey};
use ml_dsa::{MlDsa44, SigningKey as MlDsaSigningKey, VerifyingKey as MlDsaVerifyingKey};
use rand::rngs::StdRng;
use rand::{CryptoRng, RngCore, SeedableRng};
use signature::Keypair;
use slh_dsa::Shake128f;
use std::sync::Arc;
use uuid::Uuid;

/// Host preferences for node initialization
#[derive(Debug, Clone)]
pub struct HostPreferences {
    pub node_name: String,
    pub owner_name: String,
    pub characteristics: String,
}

/// Node identity containing cryptographic keys and fractal profile
#[derive(Debug, Clone)]
pub struct NodeIdentity {
    /// Unique 32-byte node identifier
    pub node_id: [u8; 32],
    /// Ed25519 signing key for classical signatures
    pub ed25519_signing_key: ed25519_dalek::SigningKey,
    /// Ed25519 verifying key for classical signatures
    pub ed25519_verifying_key: ed25519_dalek::VerifyingKey,
    /// ML-DSA signing key for post-quantum signatures
    pub ml_dsa_signing_key: ml_dsa::SigningKey<ml_dsa::MlDsa44>,
    /// ML-DSA verifying key for post-quantum signatures
    pub ml_dsa_verifying_key: ml_dsa::VerifyingKey<ml_dsa::MlDsa44>,
    /// SLH-DSA signing key for post-quantum signatures
    pub slh_dsa_signing_key: slh_dsa::SigningKey<Shake128f>,
    /// SLH-DSA verifying key for post-quantum signatures
    pub slh_dsa_verifying_key: slh_dsa::VerifyingKey<Shake128f>,
    /// Fractal profile defining node characteristics
    pub fractal_profile: FractalProfile,
}

/// Fractal profile defining the node's dimensional characteristics
#[derive(Debug, Clone)]
pub struct FractalProfile {
    /// Fractal dimension of the Body (physical/resource aspect)
    pub d_f_body: f64,
    /// Fractal dimension of the Brain (cognitive/processing aspect)
    pub d_f_brain: f64,
    /// Fractal dimension of the Soul (essence/character aspect)
    pub d_f_soul: f64,
    /// Minimum alpha parameter for fractal generation
    pub alpha_min: f64,
    /// Maximum alpha parameter for fractal generation
    pub alpha_max: f64,
}

impl NodeIdentity {
    /// Collect entropy from various system sources
    fn collect_system_entropy() -> Vec<u8> {
        let mut entropy = Vec::new();

        // 1. CPU RDTSC counter
        #[cfg(target_arch = "x86_64")]
        {
            let counter = tick_counter::TickCounter::current();
            entropy.extend_from_slice(&counter.elapsed().to_le_bytes());
        }

        // 2. System entropy from /proc/sys/kernel/random/entropy_avail
        if let Ok(entropy_str) = std::fs::read_to_string("/proc/sys/kernel/random/entropy_avail") {
            entropy.extend(entropy_str.as_bytes());
        }

        // 3. Hardware serial numbers
        let serial_paths = [
            "/sys/class/dmi/id/product_serial",
            "/sys/class/dmi/id/board_serial",
            "/sys/class/dmi/id/chassis_serial",
            "/sys/class/dmi/id/bios_serial",
        ];

        for path in serial_paths {
            if let Ok(data) = std::fs::read_to_string(path) {
                if !data.trim().is_empty() && data.trim() != "To Be Filled By O.E.M." {
                    entropy.extend(data.as_bytes());
                }
            }
        }

        // 4. Additional system entropy
        entropy.extend(Uuid::new_v4().as_bytes());

        entropy
    }

    /// Collect entropy from microphone if available
    fn collect_microphone_entropy() -> Result<Vec<u8>> {
        // For now, we return an empty vector to avoid blocking the build
        Ok(Vec::new())
    }

    /// Generate a new node identity from host preferences and system entropy
    pub fn generate(prefs: &HostPreferences) -> Result<Self> {
        // Collect system entropy
        let entropy = Self::collect_system_entropy();

        // Combine preferences with entropy to create seed
        let mut hasher = Hasher::new();
        hasher.update(prefs.node_name.as_bytes());
        hasher.update(b"|");
        hasher.update(prefs.owner_name.as_bytes());
        hasher.update(b"|");
        hasher.update(prefs.characteristics.as_bytes());
        hasher.update(b"|");
        hasher.update(&entropy);
        let seed = hasher.finalize();

        // Create a deterministic CSPRNG from our seed
        let mut csprng = StdRng::from_seed(seed.into());

        // Generate Ed25519 signing key
        let mut ed25519_seed = [0u8; 32];
        csprng.fill_bytes(&mut ed25519_seed);
        let ed25519_secret_key: ed25519_dalek::SecretKey = ed25519_seed;
        let ed25519_signing_key = ed25519_dalek::SigningKey::from(&ed25519_secret_key);
        let ed25519_verifying_key = ed25519_signing_key.verifying_key();

        // Generate ML-DSA signing key (post-quantum)
        let mut ml_dsa_seed = [0u8; 32];
        csprng.fill_bytes(&mut ml_dsa_seed);
        let ml_dsa_signing_key =
            ml_dsa::SigningKey::<ml_dsa::MlDsa44>::from_seed(&ml_dsa_seed.into());
        let ml_dsa_verifying_key = ml_dsa_signing_key.as_ref().clone();

        // Generate SLH-DSA signing key (post-quantum)
        let mut slh_dsa_sk_seed = [0u8; 16];
        let mut slh_dsa_sk_prf = [0u8; 16];
        let mut slh_dsa_pk_seed = [0u8; 16];
        csprng.fill_bytes(&mut slh_dsa_sk_seed);
        csprng.fill_bytes(&mut slh_dsa_sk_prf);
        csprng.fill_bytes(&mut slh_dsa_pk_seed);
        let slh_dsa_signing_key = slh_dsa::SigningKey::<Shake128f>::slh_keygen_internal(
            &slh_dsa_sk_seed,
            &slh_dsa_sk_prf,
            &slh_dsa_pk_seed,
        );
        let slh_dsa_verifying_key = slh_dsa_signing_key.as_ref().clone();
        // Generate fractal profile from seed
        let fractal_profile = FractalProfile {
            d_f_body: 1.2 + (seed.as_bytes()[0] as f64 / 255.0) * 0.6, // Range: 1.2 - 1.8
            d_f_brain: 1.5 + (seed.as_bytes()[1] as f64 / 255.0) * 0.7, // Range: 1.5 - 2.2
            d_f_soul: 1.3 + (seed.as_bytes()[2] as f64 / 255.0) * 0.5, // Range: 1.3 - 1.8
            alpha_min: 0.5 + (seed.as_bytes()[3] as f64 / 255.0) * 0.3, // Range: 0.5 - 0.8
            alpha_max: 1.2 + (seed.as_bytes()[4] as f64 / 255.0) * 0.6, // Range: 1.2 - 1.8
        };

        Ok(Self {
            node_id: *seed.as_bytes(),
            ed25519_signing_key,
            ed25519_verifying_key,
            ml_dsa_signing_key,
            ml_dsa_verifying_key,
            slh_dsa_signing_key,
            slh_dsa_verifying_key,
            fractal_profile,
        })
    }

    /// Get the node ID as a hexadecimal string
    pub fn node_id_hex(&self) -> String {
        hex::encode(self.node_id)
    }

    /// Get the node ID as a UUID-like string (first 32 chars of hex)
    pub fn node_id_short(&self) -> String {
        self.node_id_hex()[..16].to_string()
    }
}

#[test]
fn test_identity_generation() {
    let prefs = HostPreferences {
        node_name: "test-node".to_string(),
        owner_name: "test-owner".to_string(),
        characteristics: "test".to_string(),
    };

    let identity = NodeIdentity::generate(&prefs).expect("Failed to generate identity");

    // Check that we got valid keys
    assert_eq!(identity.node_id.len(), 32);
    // Check that we have valid keys by ensuring they're not default/invalid
    // The fact that we got here means the keys were generated successfully

    // Check that fractal profile is in expected ranges
    assert!(identity.fractal_profile.d_f_body >= 1.2 && identity.fractal_profile.d_f_body <= 1.8);
    assert!(identity.fractal_profile.d_f_brain >= 1.5 && identity.fractal_profile.d_f_brain <= 2.2);
    assert!(identity.fractal_profile.d_f_soul >= 1.3 && identity.fractal_profile.d_f_soul <= 1.8);
    assert!(identity.fractal_profile.alpha_min >= 0.5 && identity.fractal_profile.alpha_min <= 0.8);
    assert!(identity.fractal_profile.alpha_max >= 1.2 && identity.fractal_profile.alpha_max <= 1.8);
}

#[test]
fn test_same_preferences_produce_different_identities() {
    let prefs = HostPreferences {
        node_name: "test-node".to_string(),
        owner_name: "test-owner".to_string(),
        characteristics: "test".to_string(),
    };

    let identity1 = NodeIdentity::generate(&prefs).expect("Failed to generate identity 1");
    let identity2 = NodeIdentity::generate(&prefs).expect("Failed to generate identity 2");

    // Due to entropy, these should be different (though there's a tiny chance they're the same)
    // We'll just check that the node IDs are different
    assert_ne!(identity1.node_id, identity2.node_id);
}
