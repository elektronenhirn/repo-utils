use repo_utils::repo_project_selector::select_projects;
use std::env;
use std::path::Path;

const TEST_DATA_SUBFOLDER: &str = "data/repo_project_selector";

#[test]
fn test_select_projects() {
    setup();

    assert_select_projects(
        false,
        None,
        None,
        "coffeemaker,boiler,pressureliefvalve,pot,startbutton",
    );
    assert_select_projects(
        true,
        None,
        None,
        "coffeemaker,boiler,pressureliefvalve,pot,startbutton,.repo/manifests",
    );
}

#[test]
fn test_select_projects_with_group_filter() {
    setup();

    assert_select_projects(
        false,
        Some(vec!["mechanical"]),
        None,
        "pressureliefvalve,pot",
    );
    assert_select_projects(
        true,
        Some(vec!["electrical"]),
        None,
        "boiler,startbutton,.repo/manifests",
    );
    assert_select_projects(false, Some(vec!["chemical"]), None, "");
}

#[test]
fn test_select_projects_with_manifest_filter() {
    setup();

    assert_select_projects(
        false,
        None,
        Some(vec!["libs.xml"]),
        "boiler,pressureliefvalve,pot,startbutton",
    );
    assert_select_projects(
        false,
        None,
        Some(vec!["../manifest.xml"]),
        "coffeemaker,boiler,pressureliefvalve,pot,startbutton",
    );
}

#[test]
fn test_select_projects_with_all_filters() {
    setup();

    assert_select_projects(
        false,
        Some(vec!["toplevel", "electrical"]),
        Some(vec!["libs.xml"]),
        "boiler,startbutton",
    );
    assert_select_projects(
        false,
        Some(vec!["toplevel", "electrical"]),
        Some(vec!["libs.xml", "../manifest.xml"]),
        "coffeemaker,boiler,startbutton",
    );
}

fn assert_select_projects(
    include_manifest_repo: bool,
    filter_by_groups: Option<Vec<&str>>,
    filter_by_manifest_files: Option<Vec<&str>>,
    expected_seclection: &str,
) {
    assert_eq!(
        select_projects(
            include_manifest_repo,
            filter_by_groups,
            filter_by_manifest_files,
        )
        .unwrap()
        .join(","),
        expected_seclection
    );
}

fn setup() {
    let relative_to_create_root = Path::new(file!())
        .parent()
        .unwrap()
        .join(TEST_DATA_SUBFOLDER);

    if !test_data_folder_set_as_cwd() {
        let _ = env::set_current_dir(&relative_to_create_root).map_err(|e| {
            // handling of races:
            // tests might run in parallel, so another thread might already set the cwd properly
            if !test_data_folder_set_as_cwd() {
                panic!("can't set cwd: {:?}: {}", relative_to_create_root, e);
            }
        });
    }
}

fn test_data_folder_set_as_cwd() -> bool {
    env::current_dir()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string()
        .contains(TEST_DATA_SUBFOLDER)
}
