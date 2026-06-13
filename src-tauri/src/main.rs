#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn arg_after(args: &[String], flag: &str) -> Option<String> {
    args.windows(2)
        .find(|window| window[0] == flag)
        .map(|window| window[1].clone())
}

fn args_after_3(args: &[String], flag: &str) -> Option<(String, String, String)> {
    args.windows(4)
        .find(|window| window[0] == flag)
        .map(|window| (window[1].clone(), window[2].clone(), window[3].clone()))
}

fn has_arg(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}

fn main() {
    if localstack_pro_lib::try_run_service_helper() {
        return;
    }
    let args: Vec<String> = std::env::args().collect();
    if has_arg(&args, "--localstack-start-all") {
        match localstack_pro_lib::start_all_for_cli() {
            Ok(summary) => println!("{summary}"),
            Err(err) => {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
        return;
    }
    if has_arg(&args, "--localstack-stop-all") {
        match localstack_pro_lib::stop_all_for_cli() {
            Ok(summary) => println!("{summary}"),
            Err(err) => {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
        return;
    }
    if let Some(service_id) = arg_after(&args, "--localstack-restart-service") {
        match localstack_pro_lib::restart_service_for_cli(service_id) {
            Ok(summary) => println!("{summary}"),
            Err(err) => {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
        return;
    }
    if let Some(service_id) = arg_after(&args, "--localstack-check-service") {
        match localstack_pro_lib::check_service_for_cli(service_id) {
            Ok(summary) => println!("{summary}"),
            Err(err) => {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
        return;
    }
    if let Some(host_id) = arg_after(&args, "--localstack-diagnose-host") {
        match localstack_pro_lib::diagnose_host_for_cli(host_id) {
            Ok(summary) => println!("{summary}"),
            Err(err) => {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
        return;
    }
    if let Some(host_id) = arg_after(&args, "--localstack-repair-host") {
        match localstack_pro_lib::repair_host_for_cli(host_id) {
            Ok(summary) => println!("{summary}"),
            Err(err) => {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
        return;
    }
    if let Some(kind) = arg_after(&args, "--localstack-install-db-tool") {
        match localstack_pro_lib::install_db_tool_for_cli(kind) {
            Ok(summary) => println!("{summary}"),
            Err(err) => {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
        return;
    }
    if let Some(service_id) = arg_after(&args, "--localstack-install-service") {
        match localstack_pro_lib::install_service_dependency_for_cli(service_id) {
            Ok(summary) => println!("{summary}"),
            Err(err) => {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
        return;
    }
    if let Some(database_id) = arg_after(&args, "--localstack-test-database") {
        match localstack_pro_lib::test_database_for_cli(database_id) {
            Ok(summary) => println!("{summary}"),
            Err(err) => {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
        return;
    }
    if let Some(path) = arg_after(&args, "--localstack-create-backup") {
        match localstack_pro_lib::create_backup_for_cli(path) {
            Ok(summary) => println!("{summary}"),
            Err(err) => {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
        return;
    }
    if let Some(source) = arg_after(&args, "--localstack-tail-log") {
        match localstack_pro_lib::tail_log_for_cli(source) {
            Ok(summary) => println!("{summary}"),
            Err(err) => {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
        return;
    }
    if let Some(cms_args) = args_after_3(&args, "--localstack-install-cms") {
        match localstack_pro_lib::install_cms_for_cli(cms_args.0, cms_args.1, cms_args.2) {
            Ok(summary) => println!("{summary}"),
            Err(err) => {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
        return;
    }
    if has_arg(&args, "--localstack-detect-dependencies") {
        match localstack_pro_lib::detect_dependencies_for_cli() {
            Ok(summary) => println!("{summary}"),
            Err(err) => {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
        return;
    }
    if has_arg(&args, "--localstack-health-check") {
        match localstack_pro_lib::health_check_for_cli() {
            Ok(summary) => println!("{summary}"),
            Err(err) => {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
        return;
    }
    if has_arg(&args, "--localstack-repair-environment") {
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
