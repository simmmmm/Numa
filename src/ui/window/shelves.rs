use super::*;

fn library(path: &str) -> Library {
    Library { id: 0, path: std::path::PathBuf::from(path), name: None }
}

#[test]
fn years_of_one_trip_stand_under_its_name() {

    let libraries = [
        library("/photos/Chile/2019"),
        library("/photos/Chile/2023"),
        library("/photos/Italy/2015"),
        library("/photos/Italy/2019"),
        library("/photos/Italy/2023"),
        library("/photos/Mallorca"),
    ];
    let (loose, grouped) = shelve_libraries(&libraries);

    assert_eq!(loose.iter().map(|l| l.label()).collect::<Vec<_>>(), ["Mallorca"]);
    assert_eq!(
        grouped.iter().map(|(parent, run)| (parent.as_str(), run.len())).collect::<Vec<_>>(),
        [("Chile", 2), ("Italy", 3)],
        "a folder holding one library is not a shelf"
    );

    assert_eq!(grouped[1].1.iter().map(|l| l.label()).collect::<Vec<_>>(), ["2015", "2019", "2023"]);

    assert_eq!(loose.len() + grouped.iter().map(|(_, run)| run.len()).sum::<usize>(), libraries.len());
}

#[test]
fn libraries_side_by_side_are_left_alone() {
    let libraries = [library("/photos/Japan"), library("/photos/Ommen"), library("/photos/Portugal")];
    let (loose, grouped) = shelve_libraries(&libraries);
    assert_eq!(loose.len(), 3, "three siblings of one folder are not three shelves");
    assert!(grouped.is_empty());

    let root = [library("/")];
    let (loose, grouped) = shelve_libraries(&root);
    assert_eq!(loose.len() + grouped.len(), 1);
}
