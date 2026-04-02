with open('lychee-bin/tests/cli.rs', 'r') as f:
    content = f.read()

content = content.replace('exclude = ["cargo_exclude_test_str"]\n"#,\n        )?;\n\n        let mut cmd = cargo_bin_cmd!();\n        let assert = cmd\n            .current_dir(dir.path())\n            .arg("--dump")\n            .arg("https://example.com/cargo_exclude_test_str")\n            .arg("--offline")', 'exclude = ["cargo_exclude_test_str"]\n"#,\n        )?;\n\n        let mut file = std::fs::File::create(dir.path().join("input.txt"))?;\n        std::io::Write::write_all(&mut file, b"https://example.com/cargo_exclude_test_str")?;\n\n        let mut cmd = cargo_bin_cmd!();\n        let assert = cmd\n            .current_dir(dir.path())\n            .arg("input.txt")\n            .arg("--offline")')

with open('lychee-bin/tests/cli.rs', 'w') as f:
    f.write(content)
