    use super::*;

    fn node(id: &str, edges: Vec<SpecEdge>) -> SpecNodeMeta {
        SpecNodeMeta {
            id: id.to_string(),
            node_type: "feature_requirement".to_string(),
            status: "draft".to_string(),
            title: id.to_string(),
            body_md_path: format!("spec/{id}.md"),
            terms: Vec::new(),
            hash: "0".repeat(64),
            edges,
        }
    }

    #[test]
    fn extract_title_uses_heading() {
        let title = extract_title("# Hello\n\ntext", Path::new("spec/a.md"));
        assert_eq!(title, "Hello");
    }

    #[test]
    fn extract_title_falls_back_to_filename() {
        let title = extract_title("no heading", Path::new("spec/fallback-name.md"));
        assert_eq!(title, "fallback-name");
    }

    #[test]
    fn md_to_meta_path_converts_suffix() {
        let path = md_to_meta_path(Path::new("spec/10-domain-model.md")).unwrap();
        assert_eq!(path, PathBuf::from("spec/10-domain-model.meta.json"));
    }

    #[test]
    fn sha256_hex_returns_64_chars() {
        let hash = sha256_hex(b"hello");
        assert_eq!(hash.len(), 64);
        assert_eq!(
            hash,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn next_available_id_skips_max() {
        let ids = HashSet::from([
            "SPC-001".to_string(),
            "SPC-003".to_string(),
            "SPC-010".to_string(),
        ]);
        assert_eq!(next_available_id(&ids), 11);
    }

    #[test]
    fn bfs_review_order_follows_link_types() {
        let mut map = HashMap::new();
        map.insert(
            "SPC-001".to_string(),
            node(
                "SPC-001",
                vec![
                    SpecEdge {
                        to: "SPC-002".to_string(),
                        edge_type: "depends_on".to_string(),
                        rationale: "dep".to_string(),
                        confidence: 1.0,
                        status: "confirmed".to_string(),
                    },
                    SpecEdge {
                        to: "SPC-004".to_string(),
                        edge_type: "conflicts_with".to_string(),
                        rationale: "conflict".to_string(),
                        confidence: 1.0,
                        status: "confirmed".to_string(),
                    },
                ],
            ),
        );
        map.insert("SPC-002".to_string(), node("SPC-002", Vec::new()));
        map.insert(
            "SPC-003".to_string(),
            node(
                "SPC-003",
                vec![SpecEdge {
                    to: "SPC-001".to_string(),
                    edge_type: "tests".to_string(),
                    rationale: "test".to_string(),
                    confidence: 1.0,
                    status: "confirmed".to_string(),
                }],
            ),
        );
        map.insert("SPC-004".to_string(), node("SPC-004", Vec::new()));

        let order = bfs_review_order("SPC-001", 3, &map);

        assert!(order.contains(&"SPC-001".to_string()));
        assert!(order.contains(&"SPC-002".to_string()));
        assert!(order.contains(&"SPC-003".to_string()));
        assert!(!order.contains(&"SPC-004".to_string()));
    }

    #[test]
    fn bfs_review_order_respects_depth() {
        let mut map = HashMap::new();
        map.insert(
            "SPC-001".to_string(),
            node(
                "SPC-001",
                vec![SpecEdge {
                    to: "SPC-002".to_string(),
                    edge_type: "depends_on".to_string(),
                    rationale: "dep".to_string(),
                    confidence: 1.0,
                    status: "confirmed".to_string(),
                }],
            ),
        );
        map.insert(
            "SPC-002".to_string(),
            node(
                "SPC-002",
                vec![SpecEdge {
                    to: "SPC-003".to_string(),
                    edge_type: "depends_on".to_string(),
                    rationale: "dep".to_string(),
                    confidence: 1.0,
                    status: "confirmed".to_string(),
                }],
            ),
        );
        map.insert("SPC-003".to_string(), node("SPC-003", Vec::new()));

        let depth1 = bfs_review_order("SPC-001", 1, &map);
        assert!(depth1.contains(&"SPC-001".to_string()));
        assert!(depth1.contains(&"SPC-002".to_string()));
        assert!(!depth1.contains(&"SPC-003".to_string()));
    }

    #[test]
    fn normalize_term_key_collapses_style_variants() {
        assert_eq!(normalize_term_key("User_ID"), "userid");
        assert_eq!(normalize_term_key("user-id"), "userid");
        assert_eq!(normalize_term_key("User Id"), "userid");
    }

    #[test]
    fn validate_meta_semantics_rejects_invalid_fields() {
        let meta = SpecNodeMeta {
            id: "BAD-001".to_string(),
            node_type: "unknown_type".to_string(),
            status: "unknown_status".to_string(),
            title: "".to_string(),
            body_md_path: "docs/a.txt".to_string(),
            terms: vec![],
            hash: "not-a-hash".to_string(),
            edges: vec![],
        };
        let mut lint = LintState::default();
        validate_meta_semantics(Path::new("spec/a.meta.json"), &meta, &mut lint);
        assert!(lint.errors.iter().any(|e| e.contains("invalid node id format")));
        assert!(lint.errors.iter().any(|e| e.contains("invalid node type")));
        assert!(lint.errors.iter().any(|e| e.contains("invalid node status")));
        assert!(lint.errors.iter().any(|e| e.contains("empty title")));
        assert!(lint.errors.iter().any(|e| e.contains("invalid body_md_path format")));
        assert!(lint.errors.iter().any(|e| e.contains("invalid hash format")));
    }

    #[test]
    fn score_to_confidence_is_bounded() {
        assert_eq!(score_to_confidence(0), 0.0);
        assert_eq!(score_to_confidence(2), 0.6);
        assert_eq!(score_to_confidence(20), 0.9);
    }

    #[test]
    fn split_into_chunks_splits_long_text() {
        let text = "Sentence one. Sentence two is long enough to force splitting. Sentence three keeps going with more words. Sentence four concludes.";
        let chunks = split_into_chunks(text, 40);
        assert!(chunks.len() >= 2);
        assert!(chunks.iter().all(|c| !c.trim().is_empty()));
    }

    #[test]
    fn semantic_similarity_prefers_related_text() {
        let q = semantic_vector("authorization policy");
        let related = semantic_vector("authorization rules and policy for access");
        let unrelated = semantic_vector("invoice tax and payment details");
        assert!(cosine_similarity(&q, &related) > cosine_similarity(&q, &unrelated));
    }

    #[test]
    fn ranking_boost_favors_title_phrase_match() {
        let boost = ranking_boost("checkout flow", "Checkout Flow", &[]);
        let low = ranking_boost("checkout flow", "Payment module", &[]);
        assert!(boost > low);
    }

    #[test]
    fn vector_blob_roundtrip() {
        let vec = vec![0.1, -0.5, 1.25, 3.0];
        let blob = vector_to_blob(&vec);
        let decoded = blob_to_vector(&blob).expect("decode vector");
        assert_eq!(vec.len(), decoded.len());
        for (a, b) in vec.iter().zip(decoded.iter()) {
            assert!((a - b).abs() < 1e-12);
        }
    }

    #[test]
    fn vector_json_shape() {
        let json = vector_to_json(&[0.1, 0.2, -0.3]);
        assert!(json.starts_with('['));
        assert!(json.ends_with(']'));
        assert!(json.contains(','));
    }

    #[test]
    fn normalize_query_for_fts_removes_punctuation() {
        let normalized = normalize_query_for_fts("How does auth-flow work?");
        assert_eq!(normalized, "how does auth flow work");
    }

    #[test]
    fn expand_ask_context_collects_neighbors_and_conflicts() {
        let mut map = HashMap::new();
        map.insert(
            "SPC-001".to_string(),
            SpecNodeMeta {
                id: "SPC-001".to_string(),
                node_type: "feature_requirement".to_string(),
                status: "active".to_string(),
                title: "A".to_string(),
                body_md_path: "spec/a.md".to_string(),
                terms: vec![],
                hash: "0".repeat(64),
                edges: vec![
                    SpecEdge {
                        to: "SPC-002".to_string(),
                        edge_type: "depends_on".to_string(),
                        rationale: "dep".to_string(),
                        confidence: 1.0,
                        status: "confirmed".to_string(),
                    },
                    SpecEdge {
                        to: "SPC-003".to_string(),
                        edge_type: "conflicts_with".to_string(),
                        rationale: "risk".to_string(),
                        confidence: 1.0,
                        status: "confirmed".to_string(),
                    },
                ],
            },
        );
        map.insert(
            "SPC-004".to_string(),
            SpecNodeMeta {
                id: "SPC-004".to_string(),
                node_type: "feature_requirement".to_string(),
                status: "active".to_string(),
                title: "B".to_string(),
                body_md_path: "spec/b.md".to_string(),
                terms: vec![],
                hash: "0".repeat(64),
                edges: vec![SpecEdge {
                    to: "SPC-001".to_string(),
                    edge_type: "tests".to_string(),
                    rationale: "test".to_string(),
                    confidence: 1.0,
                    status: "confirmed".to_string(),
                }],
            },
        );
        let hits = vec![SearchHit {
            id: "SPC-001".to_string(),
            title: "A".to_string(),
            path: "spec/a.md".to_string(),
            score: 0.5,
            matched_terms: vec![],
            snippet: "x".to_string(),
        }];
        let (related, conflicts) =
            expand_ask_context(&hits, &map, 10, &AskEdgeWeightConfig::default());
        assert!(related.contains(&"SPC-002".to_string()));
        assert!(related.contains(&"SPC-003".to_string()));
        assert!(related.contains(&"SPC-004".to_string()));
        assert!(conflicts.contains(&"SPC-003".to_string()));
    }

    #[test]
    fn load_runtime_config_defaults_when_missing() {
        let cfg = load_runtime_config();
        assert!(cfg.ask.neighbor_limit >= 1);
        assert!(cfg.ask.snippet_count_in_answer >= 1);
    }

    #[test]
    fn build_ask_explanations_contains_graph_neighbor_reason() {
        let mut map = HashMap::new();
        map.insert(
            "SPC-001".to_string(),
            SpecNodeMeta {
                id: "SPC-001".to_string(),
                node_type: "feature_requirement".to_string(),
                status: "active".to_string(),
                title: "Root".to_string(),
                body_md_path: "spec/root.md".to_string(),
                terms: vec![],
                hash: "0".repeat(64),
                edges: vec![SpecEdge {
                    to: "SPC-002".to_string(),
                    edge_type: "depends_on".to_string(),
                    rationale: "dep".to_string(),
                    confidence: 1.0,
                    status: "confirmed".to_string(),
                }],
            },
        );
        map.insert(
            "SPC-002".to_string(),
            SpecNodeMeta {
                id: "SPC-002".to_string(),
                node_type: "feature_requirement".to_string(),
                status: "active".to_string(),
                title: "Dep".to_string(),
                body_md_path: "spec/dep.md".to_string(),
                terms: vec![],
                hash: "0".repeat(64),
                edges: vec![],
            },
        );
        let hits = vec![SearchHit {
            id: "SPC-001".to_string(),
            title: "Root".to_string(),
            path: "spec/root.md".to_string(),
            score: 0.5,
            matched_terms: vec![],
            snippet: "root".to_string(),
        }];
        let exps = build_ask_explanations(
            "root dependency",
            &hits,
            &["SPC-002".to_string()],
            &map,
            &AskEdgeWeightConfig::default(),
        );
        assert!(exps.iter().any(|e| e.id == "SPC-002" && e.reason.contains("graph neighbor")));
        assert!(exps.iter().any(|e| e.id == "SPC-002" && e.reason.contains("w=")));
    }
