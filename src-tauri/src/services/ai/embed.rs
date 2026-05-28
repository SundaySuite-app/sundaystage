//! Phase 11.1 — semantic search math (engine).
//!
//! "That song about grace we did last Christmas" should find the song even
//! without a keyword match. The plan: embed (title + author + sample lyrics) on
//! edit, cache locally, and at query time blend cosine similarity with the FTS5
//! lexical rank. This module is the pure, testable math: cosine similarity and
//! the lexical/semantic blend.
//!
//! Deferred: the embedding *provider*. Anthropic has no public embeddings API,
//! so a real build wires Voyage AI or a local model behind a feature flag and a
//! `bible_verse`-style cached-embedding table. The blend below is provider-
//! agnostic and ready for whichever embedder ships.

/// Cosine similarity of two equal-length vectors. Returns 0 for mismatched or
/// zero-magnitude inputs (so a missing embedding never dominates ranking).
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

/// Blend a normalized lexical score (FTS rank mapped to 0..1, higher = better)
/// with a semantic similarity (0..1). `semantic_weight` in 0..1 trades off
/// between them. Free tier passes `semantic_weight = 0` (lexical only).
pub fn blend(lexical: f32, semantic: f32, semantic_weight: f32) -> f32 {
    let w = semantic_weight.clamp(0.0, 1.0);
    (1.0 - w) * lexical.clamp(0.0, 1.0) + w * semantic.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_vectors_are_maximally_similar() {
        let v = [0.2, 0.5, 0.8];
        assert!((cosine_similarity(&v, &v) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn orthogonal_vectors_are_zero() {
        assert!(cosine_similarity(&[1.0, 0.0], &[0.0, 1.0]).abs() < 1e-6);
    }

    #[test]
    fn mismatched_or_zero_inputs_are_safe() {
        assert_eq!(cosine_similarity(&[1.0, 2.0], &[1.0]), 0.0);
        assert_eq!(cosine_similarity(&[], &[]), 0.0);
        assert_eq!(cosine_similarity(&[0.0, 0.0], &[1.0, 1.0]), 0.0);
    }

    #[test]
    fn blend_respects_weight() {
        // Lexical-only (free tier).
        assert!((blend(0.8, 0.2, 0.0) - 0.8).abs() < 1e-6);
        // Fully semantic.
        assert!((blend(0.8, 0.2, 1.0) - 0.2).abs() < 1e-6);
        // Even mix.
        assert!((blend(1.0, 0.0, 0.5) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn blend_clamps_out_of_range_inputs() {
        assert!((blend(2.0, -1.0, 0.0) - 1.0).abs() < 1e-6);
        assert!((blend(2.0, 5.0, 2.0) - 1.0).abs() < 1e-6);
    }
}
