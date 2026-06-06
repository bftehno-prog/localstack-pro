#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if localstack_pro_lib::try_run_service_helper() {
        return;
    }
    if std::env::args().any(|arg| arg == "--localstack-start-all") {
        match localstack_pro_lib::start_all_for_cli() {
            Ok(summary) => println!("{summary}"),
            Err(err) => {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
        return;
    }
    if std::env::args().any(|arg| arg == "--localstack-stop-all") {
        match localstack_pro_lib::stop_all_for_cli() {
            Ok(summary) => println!("{summary}"),
            Err(err) => {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
        return;
    }
    if let Some(service_id) = std::env::args()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|window| window[0] == "--localstack-restart-service")
        .map(|window| window[1].clone())
    {
        match localstack_pro_lib::restart_service_for_cli(service_id) {
            Ok(summary) => println!("{summary}"),
            Err(err) => {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
        return;
    }
    if let Some(service_id) = std::env::args()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|window| window[0] == "--localstack-check-service")
        .map(|window| window[1].clone())
    {
        match localstack_pro_lib::check_service_for_cli(service_id) {
            Ok(summary) => println!("{summary}"),
            Err(err) => {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
        return;
    }
    if let Some(host_id) = std::env::args()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|window| window[0] == "--localstack-diagnose-host")
        .map(|window| window[1].clone())
    {
        match localstack_pro_lib::diagnose_host_for_cli(host_id) {
            Ok(summary) => println!("{summary}"),
            Err(err) => {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
        return;
    }
    if let Some(host_id) = std::env::args()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|window| window[0] == "--localstack-repair-host")
        .map(|window| window[1].clone())
    {
        match localstack_pro_lib::repair_host_for_cli(host_id) {
            Ok(summary) => println!("{summary}"),
            Err(err) => {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
        return;
    }
    if let Some(kind) = std::env::args()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|window| window[0] == "--localstack-install-db-tool")
        .map(|window| window[1].clone())
    {
        match localstack_pro_lib::install_db_tool_for_cli(kind) {
            Ok(summary) => println!("{summary}"),
            Err(err) => {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
        return;
    }
    if let Some(service_id) = std::env::args()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|window| window[0] == "--localstack-install-service")
        .map(|window| window[1].clone())
    {
        match localstack_pro_lib::install_service_dependency_for_cli(service_id) {
            Ok(summary) => println!("{summary}"),
            Err(err) => {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
        return;
    }
    if let Some(database_id) = std::env::args()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|window| window[0] == "--localstack-test-database")
        .map(|window| window[1].clone())
    {
        match localstack_pro_lib::test_database_for_cli(database_id) {
            Ok(summary) => println!("{summary}"),
            Err(err) => {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
        return;
    }
    if let Some(path) = std::env::args()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|window| window[0] == "--localstack-create-backup")
        .map(|window| window[1].clone())
    {
        match localstack_pro_lib::create_backup_for_cli(path) {
            Ok(summary) => println!("{summary}"),
            Err(err) => {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
        return;
    }
    if let Some(source) = std::env::args()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|window| window[0] == "--localstack-tail-log")
        .map(|window| window[1].clone())
    {
        match localstack_pro_lib::tail_log_for_cli(source) {
            Ok(summary) => println!("{summary}"),
            Err(err) => {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
        return;
    }
    if let Some(args) = std::env::args()
        .collect::<Vec<_>>()
        .windows(4)
        .find(|window| window[0] == "--localstack-install-cms")
        .map(|window| (window[1].clone(), window[2].clone(), window[3].clone()))
    {
        match localstack_pro_lib::install_cms_for_cli(args.0, args.1, args.2) {
            Ok(summary) => println!("{summary}"),
            Err(err) => {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
        return;
    }
    if std::env::args().any(|arg| arg == "--localstack-detect-dependencies") {
        match localstack_pro_lib::detect_dependencies_for_cli() {
            Ok(summary) => println!("{summary}"),
            Err(err) => {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
        return;
    }
    if std::env::args().any(|arg| arg == "--localstack-health-check") {
        match localstack_pro_lib::health_check_for_cli() {
            Ok(summary) => println!("{summary}"),
            Err(err) => {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
        return;
    }
    if std::env::args().any(|arg| arg == "--localstack-repair-environment") {
        match localstack_pro_lib::repair_environment_for_cli() {
            Ok(summary) => println!("{summary}"),
            Err(err) => {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
        return;
    }
    localstack_pro_lib::run();
}
