pub mod model;
pub mod topo;
pub mod xml;
pub mod write;

#[cfg(test)]
mod tests {
    use crate::xml;
    use crate::topo;
    use crate::write;
    use std::path::PathBuf;

    fn sample_railml_path() -> PathBuf {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.pop(); // lib
        path.pop(); // repo root
        path.push("railML");
        path.push("IS NEST view");
        path.push("2024-07-19_railML_SimpleExample_v13_NEST_railML2.5.xml");
        path
    }

    #[test]
    fn it_works() {
        println!("Reading xml");
        let s = std::fs::read_to_string("eidsvoll.railml").unwrap();
        let railml = xml::parse_railml(&s).expect("railml parse failed");
        println!(" Found railml {:#?}", railml);

        let topo = topo::convert_railml_topo(railml).expect("topo conversion failed");
        println!(" Found topology {:#?}", topo);
        println!(" Found topology {:?}", topo);
    }

    #[test]
    fn parse_railml_25_sample() {
        let data = std::fs::read_to_string(sample_railml_path()).expect("sample railml 2.5 not found");
        let railml = xml::parse_railml(&data).expect("railml 2.5 parse failed");
        let infra = railml.infrastructure.clone().expect("infrastructure missing");

        assert!(railml.metadata.is_some(), "metadata should be parsed");
        assert!(infra.tracks.len() >= 7, "tracks should be loaded");
        assert!(
            infra.track_groups.len() >= 1,
            "track groups should be parsed"
        );
        assert!(infra.ocps.len() >= 1, "OCPs should be parsed");

        let signal_count: usize = infra
            .tracks
            .iter()
            .map(|t| t.objects.signals.len())
            .sum();
        let detector_count: usize = infra
            .tracks
            .iter()
            .map(|t| t.objects.train_detectors.len())
            .sum();
        let platform_count: usize = infra
            .tracks
            .iter()
            .map(|t| t.track_elements.platform_edges.len())
            .sum();

        assert!(signal_count > 0, "should parse signals");
        assert!(detector_count > 0, "should parse train detectors");
        assert!(platform_count > 0, "should parse platform edges");

        // topo conversion should succeed and include all connections
        let topo = topo::convert_railml_topo(railml).expect("topo conversion failed");
        assert!(
            topo.connections.len() > 0 && topo.nodes.len() > 0,
            "topology should have nodes and connections"
        );
    }

    #[test]
    fn write_roundtrip_preserves_counts() {
        let data = std::fs::read_to_string(sample_railml_path()).expect("sample railml 2.5 not found");
        let railml = xml::parse_railml(&data).expect("railml 2.5 parse failed");
        let xml = write::write_railml(&railml);
        let roundtrip = xml::parse_railml(&xml).expect("roundtrip parse failed");

        let infra1 = railml.infrastructure.unwrap();
        let infra2 = roundtrip.infrastructure.unwrap();

        assert_eq!(infra1.tracks.len(), infra2.tracks.len(), "track count should survive roundtrip");
        assert_eq!(infra1.track_groups.len(), infra2.track_groups.len(), "track groups should survive roundtrip");
        assert_eq!(infra1.ocps.len(), infra2.ocps.len(), "OCPs should survive roundtrip");
        assert_eq!(infra1.states.len(), infra2.states.len(), "states should survive roundtrip");

        let count_objects = |tracks: &[crate::model::Track]| {
            tracks.iter().fold((0usize, 0usize, 0usize), |acc, t| {
                (
                    acc.0 + t.objects.signals.len(),
                    acc.1 + t.objects.train_detectors.len(),
                    acc.2 + t.track_elements.platform_edges.len(),
                )
            })
        };

        let (s1, d1, p1) = count_objects(&infra1.tracks);
        let (s2, d2, p2) = count_objects(&infra2.tracks);
        assert_eq!(s1, s2, "signal count should survive roundtrip");
        assert_eq!(d1, d2, "detector count should survive roundtrip");
        assert_eq!(p1, p2, "platform count should survive roundtrip");

        assert!(roundtrip.metadata.is_some(), "metadata should be written and parsed");
    }
}
