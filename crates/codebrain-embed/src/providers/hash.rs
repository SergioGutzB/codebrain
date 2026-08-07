use async_trait::async_trait;

use crate::error::Result;
use crate::providers::Embedder;

/// Deterministic embedder for tests: BLAKE3 bytes → unit-length float vector.
pub struct HashEmbedder {
    dimension: usize,
}

impl HashEmbedder {
    pub fn new(dimension: usize) -> Self {
        Self {
            dimension: dimension.max(8),
        }
    }
}

#[async_trait]
impl Embedder for HashEmbedder {
    fn kind(&self) -> &'static str {
        "hash"
    }

    fn model(&self) -> &str {
        "blake3-hash"
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    fn enabled(&self) -> bool {
        true
    }

    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        Ok(texts
            .iter()
            .map(|text| hash_vector(text, self.dimension))
            .collect())
    }
}

fn hash_vector(text: &str, dimension: usize) -> Vec<f32> {
    let digest = blake3::hash(text.as_bytes());
    let bytes = digest.as_bytes();
    let mut values = Vec::with_capacity(dimension);
    let mut counter = 0_u32;
    while values.len() < dimension {
        let mut hasher = blake3::Hasher::new();
        hasher.update(bytes);
        hasher.update(&counter.to_le_bytes());
        let block = hasher.finalize();
        for chunk in block.as_bytes().chunks_exact(4) {
            if values.len() >= dimension {
                break;
            }
            let bits = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            // Map to [-1, 1]
            values.push((bits as f32 / u32::MAX as f32) * 2.0 - 1.0);
        }
        counter += 1;
    }
    let norm = values
        .iter()
        .map(|value| value * value)
        .sum::<f32>()
        .sqrt()
        .max(1e-8);
    for value in &mut values {
        *value /= norm;
    }
    values
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn produces_stable_unit_vectors() {
        let embedder = HashEmbedder::new(32);
        let first = embedder.embed_one("Greeter").await.expect("embed");
        let second = embedder.embed_one("Greeter").await.expect("embed");
        assert_eq!(first, second);
        assert_eq!(first.len(), 32);
        let norm: f32 = first.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-4);
    }

    #[tokio::test]
    async fn different_texts_differ() {
        let embedder = HashEmbedder::new(32);
        let a = embedder.embed_one("alpha").await.expect("a");
        let b = embedder.embed_one("beta").await.expect("b");
        assert_ne!(a, b);
    }
}
