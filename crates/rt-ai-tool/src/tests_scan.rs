//! Regression tests for the inline-secret scan: arrays at any depth, key
//! names that look like credentials, and non-loopback http endpoints
//! (Req 28.4, 28.6, 28.16).

use crate::tests::{at, env_ok, no_env_read, one, run, with, BASE};
use crate::{render_diagnostic, Out};
use rt_foundation::Code;
use std::cell::RefCell;
use std::io;
use std::rc::Rc;

const VALUE: &str = "ZVALUE9";

#[test]
fn secret_names_inside_arrays_and_deep_tables_are_rejected_without_echo() {
    let cases = [
        format!("x = [{{ api_key = \"{VALUE}\" }}]\n"),
        format!("[[x]]\napi_key = \"{VALUE}\"\n"),
        format!("x = {{ a = {{ b = {{ api_key = \"{VALUE}\" }} }} }}\n"),
        format!("[x.a.b]\napi_key = \"{VALUE}\"\n"),
        format!("x = [[{{ token = \"{VALUE}\" }}]]\n"),
        format!("x = [{{ a = [{{ b = [{{ secret = \"{VALUE}\" }}] }}] }}]\n"),
        format!("[[x]]\n[[x.y]]\n[x.y.z]\npassword = \"{VALUE}\"\n"),
        format!("x = [{{ AKIAIOSFODNN7EXAMPLE = \"{VALUE}\" }}]\n"),
    ];
    for tail in cases {
        let env = env_ok();
        let d = one(
            run(&format!("{BASE}{tail}"), &env),
            Code::AiConfigInlineSecret,
        );
        assert_eq!(at(&d).0, Some(10), "{tail}");
        assert_eq!(at(&d).2, None, "{tail}");
        let shown = render_diagnostic(&d);
        for echoed in [VALUE, "api_key", "token", "secret", "password", "AKIA"] {
            assert!(
                !shown.contains(echoed),
                "{echoed} echoed for {tail}: {shown}"
            );
        }
        no_env_read(&env);
    }
}

#[test]
fn credential_like_values_inside_arrays_of_tables_are_rejected() {
    let env = env_ok();
    let tail = "[[x]]\n[[x.y]]\ninner = \"sk-zzz\"\n";
    let d = one(
        run(&format!("{BASE}{tail}"), &env),
        Code::AiConfigInlineSecret,
    );
    assert_eq!(at(&d), (Some(10), Some(3), Some("x".to_owned())));
}

#[test]
fn clean_arrays_of_tables_are_not_rejected_as_secrets() {
    let text = format!("{BASE}x = [{{ a = [{{ b = 1 }}] }}]\n");
    let d = one(run(&text, &env_ok()), Code::AiConfigInvalid);
    assert_eq!(at(&d).2.as_deref(), Some("x"));
}

const KEY_CANARIES: [&str; 5] = [
    "AKIAIOSFODNN7EXAMPLE",
    "sk-abcdef123",
    "sk_abcdef123",
    "Bearer abcdef123",
    "ghp_abcdef123",
];

struct Shared(Rc<RefCell<Vec<u8>>>);

impl io::Write for Shared {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn credential_like_key_names_are_inline_secrets_and_appear_in_no_output() {
    for key in KEY_CANARIES {
        for (tail, column) in [
            (format!("\"{key}\" = 1\n"), 1),
            (format!("x = {{ \"{key}\" = 1 }}\n"), 1),
            (format!("[[x]]\n\"{key}\" = 1\n"), 3),
        ] {
            let env = env_ok();
            let d = one(
                run(&format!("{BASE}{tail}"), &env),
                Code::AiConfigInlineSecret,
            );
            assert_eq!(at(&d), (Some(10), Some(column), None), "{tail}");
            let stdout: Rc<RefCell<Vec<u8>>> = Rc::default();
            let stderr: Rc<RefCell<Vec<u8>>> = Rc::default();
            let mut out = Out::without_credential(Shared(stdout.clone()), Shared(stderr.clone()));
            out.diagnostic(&d).unwrap();
            let all = format!(
                "{}{}{:?}",
                String::from_utf8(stdout.borrow().clone()).unwrap(),
                String::from_utf8(stderr.borrow().clone()).unwrap(),
                d.primary()
            );
            assert!(!all.contains(key), "{key} echoed: {all}");
            no_env_read(&env);
        }
    }
}

#[test]
fn ordinary_unknown_keys_stay_invalid_config() {
    let d = one(
        run(&format!("{BASE}colour = 1\n"), &env_ok()),
        Code::AiConfigInvalid,
    );
    assert_eq!(at(&d).2.as_deref(), Some("colour"));
}

#[test]
fn http_to_a_non_loopback_host_is_insecure_even_for_unspecified_addresses() {
    for url in [
        "http://[::]",
        "http://[::]:8080",
        "http://0.0.0.0",
        "http://0.0.0.0:80/x",
        "http://[::ffff:127.0.0.1]",
        "http://[fe80::1]",
    ] {
        let env = env_ok();
        let d = one(
            run(&with(("https://api.openai.com", url)), &env),
            Code::AiConfigEndpointInsecure,
        );
        assert_eq!(at(&d).2.as_deref(), Some("endpoint"), "{url}");
        no_env_read(&env);
    }
}

#[test]
fn unbracketed_double_colon_host_is_a_malformed_endpoint() {
    let d = one(
        run(&with(("https://api.openai.com", "http://::")), &env_ok()),
        Code::AiConfigInvalid,
    );
    assert_eq!(at(&d).2.as_deref(), Some("endpoint"));
}

#[test]
fn field_check_never_echoes_a_credential_like_unknown_key() {
    use crate::locate::Locator;
    use crate::raw::RawConfig;
    use crate::reader::Reader;
    use rt_foundation::InputId;

    let text = "AKIAIOSFODNN7EXAMPLE = 1\n\"my_key\" = 2\ncolour = 3\n";
    let document: toml_edit::DocumentMut = text.parse().unwrap();
    let input = InputId::new("ai.toml").unwrap();
    let locator = Locator::new(&input, text);
    let reader = Reader {
        table: document.as_table(),
        locator: &locator,
        prefix: "",
    };
    let found = RawConfig::read(&reader).unwrap_err();
    let codes: Vec<_> = found.iter().map(|d| d.code()).collect();
    assert_eq!(
        codes,
        [
            Code::AiConfigInlineSecret,
            Code::AiConfigInlineSecret,
            Code::AiConfigInvalid
        ]
    );
    let shown: String = found.iter().map(render_diagnostic).collect();
    assert!(
        !shown.contains("AKIA") && !shown.contains("my_key"),
        "{shown}"
    );
    assert!(shown.contains("colour"));
}
