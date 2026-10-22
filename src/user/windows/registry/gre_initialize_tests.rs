use super::*;

fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}
fn registry() -> Registry {
    Registry::selected(
        (0..=u16::MAX)
            .map(|u| if (97..=122).contains(&u) { u - 32 } else { u })
            .collect(),
        vec![],
    )
    .unwrap()
}

#[test]
fn native_gre_initialize_registry_absence_is_confined_to_selected_root() {
    let original = registry();
    let selected = original
        .clone()
        .with_tree(TreeRoot::GreInitialize, None)
        .unwrap();
    for suffix in ["", "\\", "\\\\child\\nested\\"] {
        let path = format!("{GRE_INITIALIZE_KEY}{suffix}");
        assert!(matches!(original.lookup(&units(&path)), Lookup::Unselected));
        assert!(matches!(selected.lookup(&units(&path)), Lookup::Missing));
        assert!(matches!(
            selected.lookup(&units(&path.to_ascii_lowercase())),
            Lookup::Missing
        ));
    }
    for suffix in ["Suffix", "/child", "\0\\child"] {
        assert!(matches!(
            selected.lookup(&units(&format!("{GRE_INITIALIZE_KEY}{suffix}"))),
            Lookup::Unselected
        ));
    }
    for path in [
        "\\Registry\\Machine\\Software\\Microsoft\\Windows NT\\CurrentVersion",
        "\\Registry\\Machine\\Software\\Microsoft\\Windows NT\\CurrentVersion\\GRE_InitializePrefix",
        "\\Registry\\Machine\\Software\\Microsoft\\Windows NT\\CurrentVersion\\GRE_Initializes",
        NLS_SORTING_VERSIONS_KEY,
    ] {
        assert!(matches!(selected.lookup(&units(path)), Lookup::Unselected));
    }
}

#[test]
fn native_gre_initialize_registry_present_tree_owns_raw_data_and_descendants() {
    let registry = registry()
        .with_tree(
            TreeRoot::GreInitialize,
            Some(KeyTree {
                values: vec![],
                children: vec![(
                    units("literal.child"),
                    KeyTree {
                        values: vec![Value {
                            name: units("Raw"),
                            kind: 3,
                            data: vec![0, 255, 17],
                        }],
                        children: vec![],
                    },
                )],
            }),
        )
        .unwrap();
    let root = registry.key(&units(GRE_INITIALIZE_KEY)).unwrap();
    let child = registry
        .key(&units(&format!("{GRE_INITIALIZE_KEY}\\LITERAL.CHILD\\")))
        .unwrap();
    assert_eq!(root.children, 1);
    assert!(Arc::ptr_eq(&root.upcase, &child.upcase));
    drop(registry);
    drop(root);
    let value = child.value(&units("RAW")).unwrap();
    assert_eq!((value.kind, value.data.as_slice()), (3, &[0, 255, 17][..]));
    assert!(matches!(child.relative(&units("absent")), Lookup::Missing));
}

#[test]
fn native_gre_initialize_registry_selection_rejects_duplicates_and_excess_values() {
    assert!(
        Registry::default()
            .with_tree(TreeRoot::GreInitialize, None)
            .is_err()
    );
    let selected = registry().with_tree(TreeRoot::GreInitialize, None).unwrap();
    assert!(
        selected
            .with_tree(TreeRoot::GreInitialize, Some(KeyTree::default()))
            .is_err()
    );
    let full = registry()
        .with_ifeo(KeyTree {
            values: (0..MAX_VALUES)
                .map(|i| Value {
                    name: units(&i.to_string()),
                    kind: 3,
                    data: vec![],
                })
                .collect(),
            children: vec![],
        })
        .unwrap();
    assert!(
        full.with_tree(
            TreeRoot::GreInitialize,
            Some(KeyTree {
                values: vec![Value {
                    name: units("extra"),
                    kind: 3,
                    data: vec![1]
                }],
                children: vec![],
            })
        )
        .is_err()
    );
}

pub(crate) const CAPTURES: [&str; 4] = [
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/specifications/windows/native-gre-initialize/native/native-gre-initialize-v1-four-views-arm64.log"
    )),
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/specifications/windows/native-gre-initialize/native/native-gre-initialize-v1-four-views-x64.log"
    )),
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/specifications/windows/native-gre-initialize/native/native-gre-initialize-v1-four-views-x86.log"
    )),
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/specifications/windows/native-gre-initialize/native/native-gre-initialize-v1-four-views-x86-laa.log"
    )),
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CapturedNode {
    pub(crate) path: Vec<u16>,
    pub(crate) values: Vec<Value>,
    pub(crate) children: Vec<Vec<u16>>,
    count: usize,
    subkeys: usize,
    time: u64,
}
pub(crate) fn field<'a>(row: &'a str, key: &str) -> &'a str {
    row.split_ascii_whitespace()
        .find_map(|word| word.strip_prefix(key))
        .unwrap()
}
fn bytes(text: &str) -> Vec<u8> {
    assert_eq!(text.len() % 2, 0);
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16).unwrap())
        .collect()
}
fn words(text: &str) -> Vec<u16> {
    let raw = bytes(text);
    assert_eq!(raw.len() % 2, 0);
    raw.chunks_exact(2)
        .map(|raw| u16::from_le_bytes(raw.try_into().unwrap()))
        .collect()
}
pub(crate) fn capture(log: &str, sample: &str, view: &str) -> Vec<CapturedNode> {
    let mut nodes = BTreeMap::<Vec<u16>, CapturedNode>::new();
    for row in log.lines().filter(|row| {
        row.starts_with("TREE ") || row.starts_with("VALUE ") || row.starts_with("CHILD ")
    }) {
        if field(row, "sample=") != sample || field(row, "view=") != view {
            continue;
        }
        if row.starts_with("TREE ") {
            assert_eq!(field(row, "status="), "00000000");
            let path = words(field(row, "path="));
            let node = CapturedNode {
                path: path.clone(),
                values: vec![],
                children: vec![],
                count: field(row, "values=").parse().unwrap(),
                subkeys: field(row, "children=").parse().unwrap(),
                time: u64::from_str_radix(field(row, "time="), 16).unwrap(),
            };
            assert!(nodes.insert(path, node).is_none());
        } else if row.starts_with("VALUE ") {
            let node = nodes.get_mut(&words(field(row, "path="))).unwrap();
            let data = bytes(field(row, "data="));
            assert_eq!(data.len(), field(row, "bytes=").parse::<usize>().unwrap());
            node.values.push(Value {
                name: words(field(row, "name=")),
                kind: field(row, "kind=").parse().unwrap(),
                data,
            });
        } else {
            nodes
                .get_mut(&words(field(row, "parent=")))
                .unwrap()
                .children
                .push(words(field(row, "name=")));
        }
    }
    assert_eq!(nodes.len(), 3);
    for node in nodes.values() {
        assert_eq!(node.count, node.values.len());
        assert_eq!(node.subkeys, node.children.len());
    }
    nodes.into_values().collect()
}
pub(crate) fn captured_tree(nodes: &[CapturedNode]) -> KeyTree {
    fn build(nodes: &[CapturedNode], path: &[u16]) -> KeyTree {
        let node = nodes.iter().find(|node| node.path == path).unwrap();
        KeyTree {
            values: node.values.clone(),
            children: node
                .children
                .iter()
                .map(|name| {
                    let path = path
                        .iter()
                        .copied()
                        .chain(Some(92))
                        .chain(name.iter().copied())
                        .collect::<Vec<_>>();
                    (name.clone(), build(nodes, &path))
                })
                .collect(),
        }
    }
    build(nodes, &units(GRE_INITIALIZE_KEY))
}
#[test]
fn native_gre_initialize_registry_preserves_all_raw_values_and_complete_view_captures() {
    let expected = capture(CAPTURES[0], "0", "000");
    assert_eq!(
        expected.iter().map(|node| node.values.len()).sum::<usize>(),
        7
    );
    assert_eq!(
        expected
            .iter()
            .flat_map(|node| &node.values)
            .map(|value| value.name.len() * 2 + value.data.len())
            .sum::<usize>(),
        322
    );
    for log in CAPTURES {
        for sample in ["0", "1"] {
            for view in ["000", "100", "200"] {
                let nodes = capture(log, sample, view);
                assert_eq!(nodes, expected);
                let registry = registry()
                    .with_tree(TreeRoot::GreInitialize, Some(captured_tree(&nodes)))
                    .unwrap();
                for node in &nodes {
                    let key = registry.key(&node.path).unwrap();
                    assert_eq!(key.path, node.path);
                    assert_eq!(key.children as usize, node.children.len());
                    assert_eq!(key.values.len(), node.values.len());
                    for value in &node.values {
                        assert_eq!(key.value(&value.name), Some(value));
                    }
                }
            }
        }
    }
}
