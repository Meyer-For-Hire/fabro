use fabro_graphviz::graph::{AttrValue, Edge, Graph, Node};

pub(crate) fn minimal_graph() -> Graph {
    let mut g = Graph::new("test");
    let mut start = Node::new("start");
    start.attrs.insert(
        "shape".to_string(),
        AttrValue::String("Mdiamond".to_string()),
    );
    g.nodes.insert("start".to_string(), start);

    let mut exit = Node::new("exit");
    exit.attrs.insert(
        "shape".to_string(),
        AttrValue::String("Msquare".to_string()),
    );
    g.nodes.insert("exit".to_string(), exit);

    g.edges.push(Edge::new("start", "exit"));
    g
}
