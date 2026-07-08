fn main() {
    let config = ham_desktop::desktop_runtime_config();
    println!(
        "{} desktop foundation: frontend={} release_requires_dev_server={}",
        config.app_name,
        config.frontend_dist_dir.display(),
        config.release_requires_dev_server
    );
}
