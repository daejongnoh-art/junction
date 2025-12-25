pub mod model;
pub mod topo;
pub mod xml;

#[cfg(test)]
mod tests {
    use crate::xml;
    use crate::topo;
    use std::path::PathBuf;

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
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.pop(); // lib
        path.pop(); // repo root
        path.push("railML");
        path.push("IS NEST view");
        path.push("2024-07-19_railML_SimpleExample_v13_NEST_railML2.5.xml");

        let data = std::fs::read_to_string(path).expect("sample railml 2.5 not found");
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
}
