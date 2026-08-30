//! A6 tests.
//!
//! Three things have to be proved, and only one of them is arithmetic:
//!
//! 1. **Nothing that could be content can get out.** Swept, not sampled: every
//!    reachable combination of the record's enums is formatted, parsed, turned
//!    into a ring entry, and every byte is checked against a fixed character
//!    class — including the one field whose value is not a `match` over an enum.
//! 2. **The consent gate holds.** Driven through the REAL `client::drain`, not
//!    through a re-implementation of what it does.
//! 3. **A real hard crash is actually captured, and the process still dies of
//!    it.** Driven by faulting a real child process for real. This is the seam
//!    every part of this module exists for, and it is the one no amount of unit
//!    testing can stand in for.

use std::path::Path;

use super::*;

// ─────────────────────────────────────────────────────────────────────────────
//   Every record this module can produce
// ─────────────────────────────────────────────────────────────────────────────

/// Every reachable [`NativeCrash`], with a spread of codes and offsets.
fn every_crash() -> Vec<NativeCrash> {
    let sites = [
        Site::App { offset: 0 },
        Site::App { offset: 0x1a2b3c },
        Site::App { offset: u64::MAX },
        Site::Foreign,
        Site::Unknown,
    ];
    let codes = [0i64, 1, 2, -1, i64::MIN, i64::MAX, 0xC000_0005];
    let mut out = Vec::new();
    for signal in NativeSignal::ALL {
        for fault in FaultAddress::ALL {
            for site in sites {
                for thread in ThreadRole::ALL {
                    for code in codes {
                        out.push(NativeCrash {
                            signal,
                            code,
                            fault,
                            site,
                            thread,
                        });
                    }
                }
            }
        }
    }
    out
}

fn line_of(crash: &NativeCrash, version: &str) -> String {
    let mut buf = [0u8; RECORD_MAX_BYTES];
    let n = crash
        .write_line(version, &mut buf)
        .expect("every reachable record fits in RECORD_MAX_BYTES");
    String::from_utf8(buf[..n].to_vec()).expect("ASCII only")
}

#[test]
fn every_record_round_trips_through_the_file_format() {
    // The pending file is written by a dying process and read by the NEXT one,
    // possibly a different build. If the two halves disagree about a single
    // token, the crash we went to all this trouble to capture is discarded on
    // the next launch — silently, since nobody is watching.
    for crash in every_crash() {
        let line = line_of(&crash, "0.7.0");
        let (back, version) = NativeCrash::parse_line(&line)
            .unwrap_or_else(|| panic!("did not round trip: {line:?}"));
        assert_eq!(back, crash, "{line:?}");
        assert_eq!(version, "0.7.0", "{line:?}");
    }
}

#[test]
fn the_record_can_only_spell_its_own_vocabulary() {
    // Law 2 as a character class. Every byte of every reachable record has to be
    // a lowercase letter, a digit, or one of six punctuation marks — which is
    // structurally unable to spell a path, a song title or a name. Adversarial
    // versions are included because `ver=` is the ONE field whose value is not a
    // `match` over an enum.
    let versions = [
        "0.7.0",
        "0.7.0-beta.1",
        // Things a hand-edited Cargo.toml, a patched build or a hostile
        // environment could put there. None of them may survive.
        "/Users/ola/Musikk/salme.mp4",
        "C:\\Users\\Ola\\Sanger\\salme.txt",
        "Ola Nordmann",
        "0.7.0 sig=segv ver=oops",
        "0.7.0\nsundaystage-native-crash v1",
        "Deg være ære",
        "",
        // The shape check has to survive near-misses too: a string that is MOSTLY
        // a version must not be salvaged into one.
        "0.7.0.salme",
        "0.7.0-Ola",
        "0.7",
        "99999.1.1",
    ];
    for crash in every_crash() {
        for version in versions {
            let line = line_of(&crash, version);
            for (i, b) in line.bytes().enumerate() {
                let ok = b.is_ascii_lowercase()
                    || b.is_ascii_uppercase()
                    || b.is_ascii_digit()
                    || matches!(b, b' ' | b'=' | b'+' | b'.' | b'-' | b'\n');
                assert!(
                    ok,
                    "byte {i} ({:?}) escaped the vocabulary in {line:?}",
                    b as char
                );
            }
            // The specific shapes law 2 names, spelled out so a future widening
            // of the character class cannot quietly let one back in.
            assert!(!line.contains('/'), "{line:?}");
            assert!(!line.contains('\\'), "{line:?}");
            assert!(!line.contains('~'), "{line:?}");
            assert!(!line.contains(':'), "{line:?}");
            // Not one word of the adversarial inputs may survive — not even with
            // the separators removed, which is what a character FILTER would
            // have left behind.
            for word in ["Users", "ola", "Musikk", "salme", "Nordmann", "Ola", "oops"] {
                assert!(
                    !line.contains(word),
                    "{word:?} survived into {line:?} — the version is being \
                     salvaged instead of shape-checked"
                );
            }
            // Exactly one line: a record that could contain a newline could
            // forge a second record.
            assert_eq!(line.matches('\n').count(), 1, "{line:?}");
            assert!(line.ends_with('\n'), "{line:?}");
        }
    }
}

#[test]
fn the_ring_entry_a_native_crash_becomes_carries_no_free_text() {
    // The same sweep one layer further out: what actually reaches `crashes[]`
    // on the wire. `message`, `location` and `task` are the three free-text
    // fields the Worker will accept a string in, so they are the three that have
    // to be provably generated rather than merely scrubbed.
    for crash in every_crash() {
        let entry = crash.to_entry("0.7.0-beta.1", 1_800_000_000_000);
        assert_eq!(entry.kind, CrashKind::Other);
        assert!(!entry.backtrace_present, "we capture no backtrace, ever");
        assert_eq!(entry.app_version, "0.7.0-beta.1");

        let fields: Vec<&str> = [
            Some(entry.message.as_str()),
            entry.location.as_deref(),
            entry.task.as_deref(),
        ]
        .into_iter()
        .flatten()
        .collect();

        for field in fields {
            for b in field.bytes() {
                assert!(
                    b.is_ascii_alphanumeric()
                        || matches!(b, b' ' | b'=' | b'+' | b'.' | b'-' | b':'),
                    "{field:?} left the vocabulary"
                );
            }
            // The Worker's own last line of defence, mirrored: a field that
            // matched `ABSOLUTE_PATH_RE` would be refused with a 400, which the
            // outbox drops permanently and without retry.
            assert!(!field.contains('/'), "{field:?}");
            assert!(!field.contains('\\'), "{field:?}");
        }

        assert!(entry.message.starts_with("native crash: sig="));
        // The one thing an aggregate groups on has to survive the caps.
        assert!(entry.message.chars().count() <= crate::telemetry::MESSAGE_MAX_CHARS + 1);
        if let Some(loc) = entry.location.as_deref() {
            assert!(loc.chars().count() <= crate::telemetry::LOCATION_MAX_CHARS + 1);
        }
        assert!(entry.task.as_deref().unwrap_or("").ends_with("-thread"));
    }
}

#[test]
fn an_unknown_site_leaves_the_location_empty_rather_than_guessing() {
    let unknown = NativeCrash {
        signal: NativeSignal::Abort,
        code: 0,
        fault: FaultAddress::Unknown,
        site: Site::Unknown,
        thread: ThreadRole::Unknown,
    };
    assert_eq!(unknown.to_entry("0.7.0", 1).location, None);

    let foreign = NativeCrash {
        site: Site::Foreign,
        ..unknown
    };
    assert_eq!(
        foreign.to_entry("0.7.0", 1).location.as_deref(),
        Some("foreign")
    );

    let ours = NativeCrash {
        site: Site::App { offset: 0x1a2b3c },
        ..unknown
    };
    assert_eq!(
        ours.to_entry("0.7.0", 1).location.as_deref(),
        Some("app+0x1a2b3c")
    );
}

// ─────────────────────────────────────────────────────────────────────────────
//   The parser, against files it will actually meet
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn a_torn_or_hand_edited_record_is_refused_rather_than_half_believed() {
    for raw in [
        "",
        "   \n",
        "sundaystage-native-crash",
        // Truncated by a process that died mid-write.
        "sundaystage-native-crash v1 sig=segv code=1 fault=nu",
        // A future format version this build cannot read.
        "sundaystage-native-crash v2 sig=segv code=1 fault=null site=foreign \
         thread=main-thread ver=0.8.0",
        // Somebody else's file that happens to be here.
        "hello world",
        // A vocabulary this build does not have.
        "sundaystage-native-crash v1 sig=lyrics code=1 fault=null site=foreign \
         thread=main-thread ver=0.7.0",
        // A site that is not one of the three shapes.
        "sundaystage-native-crash v1 sig=segv code=1 fault=null \
         site=/Users/ola/song.txt thread=main-thread ver=0.7.0",
        // A code that is not a number.
        "sundaystage-native-crash v1 sig=segv code=x fault=null site=foreign \
         thread=main-thread ver=0.7.0",
        // A missing field.
        "sundaystage-native-crash v1 sig=segv code=1 site=foreign thread=main-thread ver=0.7.0",
        // A key with no value.
        "sundaystage-native-crash v1 sig=segv code=1 fault=null site=foreign thread ver=0.7.0",
    ] {
        assert_eq!(
            NativeCrash::parse_line(raw).map(|(c, _)| c),
            None,
            "accepted {raw:?}"
        );
    }
}

#[test]
fn a_record_from_a_build_that_knows_more_keeps_the_fields_this_one_understands() {
    // Forward compatibility in the direction it actually happens: an install
    // hard-crashes, auto-updates, and the NEW build adopts the OLD build's file.
    // The reverse (a newer writer, an older reader) is what an unknown KEY is,
    // and dropping the whole record over it would throw away a crash we can
    // read most of.
    let line = "sundaystage-native-crash v1 sig=segv code=1 fault=low site=app+0x2a \
                thread=other-thread ver=0.9.0 cores=8\n";
    let (crash, version) = NativeCrash::parse_line(line).expect("parsed");
    assert_eq!(crash.signal, NativeSignal::Segv);
    assert_eq!(crash.fault, FaultAddress::LowPage);
    assert_eq!(crash.site, Site::App { offset: 42 });
    assert_eq!(crash.thread, ThreadRole::Other);
    assert_eq!(version, "0.9.0");
}

#[test]
fn a_version_on_disk_is_shape_checked_again_on_the_way_in() {
    // The writer's guarantee is not the reader's: the pending file sits on a
    // disk anything can edit, and it is read back into a field that goes on the
    // wire. Note what "checked" has to mean — the FIRST draft of this module
    // filtered characters instead, which turns `/Users/ola/x` into `Usersolax`
    // and calls it clean.
    for hostile in [
        "/Users/ola/x",
        "C:\\Sanger\\salme.txt",
        "0.7.0.salme",
        "0.7.0-Ola",
        "",
    ] {
        let line = format!(
            "sundaystage-native-crash v1 sig=abort code=0 fault=unknown site=unknown \
             thread=unknown-thread ver={hostile}\n"
        );
        let (_, version) = NativeCrash::parse_line(&line).expect("parsed");
        assert_eq!(version, "unknown", "salvaged {hostile:?}");
    }

    // …and a real version still survives untouched.
    let good = "sundaystage-native-crash v1 sig=abort code=0 fault=unknown site=unknown \
                thread=unknown-thread ver=0.7.0-beta.1\n";
    assert_eq!(
        NativeCrash::parse_line(good).expect("parsed").1,
        "0.7.0-beta.1"
    );
}

#[test]
fn the_version_shape_check_accepts_versions_and_nothing_else() {
    for good in ["0.0.0", "0.7.0", "1.2.3", "0.7.0-beta.1", "9999.1.1-rc.2"] {
        assert!(version_is_wellformed(good), "rejected {good:?}");
    }
    for bad in [
        "",
        "0.7",
        "0.7.0.1",
        "99999.1.1",
        "0.7.0-",
        "0.7.0-Beta",
        "0.7.0-beta 1",
        "0.7.0+build",
        "v0.7.0",
        "/Users/ola",
        "0.7.0-averylongprerelease",
        "Deg være ære",
    ] {
        assert!(!version_is_wellformed(bad), "accepted {bad:?}");
    }
    // The one that actually ships must pass, or every real record says
    // "unknown".
    assert!(version_is_wellformed(crate::telemetry::app_version()));
}

// ─────────────────────────────────────────────────────────────────────────────
//   The allocation-free writer
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn a_buffer_too_small_refuses_rather_than_writing_half_a_record() {
    // A truncated record parses as nothing on the next startup, so the failure
    // has to be visible AT WRITE TIME as `None` — not as a shorter line.
    let crash = NativeCrash {
        signal: NativeSignal::StackOverflow,
        code: -2_147_483_648,
        fault: FaultAddress::NonNull,
        site: Site::App { offset: u64::MAX },
        thread: ThreadRole::Unknown,
    };
    let full = line_of(&crash, "0.7.0-beta.10");
    for size in 0..full.len() {
        let mut buf = vec![0u8; size];
        assert_eq!(
            crash.write_line("0.7.0-beta.10", &mut buf),
            None,
            "a {size}-byte buffer must refuse"
        );
    }
    let mut buf = vec![0u8; full.len()];
    assert_eq!(
        crash.write_line("0.7.0-beta.10", &mut buf),
        Some(full.len())
    );

    // And the worst case really does fit the stack buffer the handler uses.
    assert!(
        full.len() < RECORD_MAX_BYTES,
        "worst case {} vs {RECORD_MAX_BYTES}",
        full.len()
    );
}

#[test]
fn the_hand_rolled_integer_writer_agrees_with_the_formatter_it_replaces() {
    // `format!` allocates, so the handler cannot use it — which means this
    // arithmetic is hand-written and therefore capable of being wrong. `i64::MIN`
    // in particular has no positive counterpart, so the naive `-v` overflows.
    for v in [
        0i64,
        1,
        -1,
        9,
        10,
        -10,
        99,
        1_000_000,
        i64::MAX,
        i64::MIN,
        0xC000_0005,
        -2_147_483_648,
    ] {
        let mut buf = [0u8; 64];
        let mut c = Cursor::new(&mut buf);
        c.int(v);
        let n = c.finish().expect("fits");
        assert_eq!(std::str::from_utf8(&buf[..n]), Ok(v.to_string().as_str()));
    }

    for v in [0u64, 1, 15, 16, 0x1a2b3c, u64::MAX] {
        let mut buf = [0u8; 64];
        let mut c = Cursor::new(&mut buf);
        c.hex(v);
        let n = c.finish().expect("fits");
        assert_eq!(
            std::str::from_utf8(&buf[..n]),
            Ok(format!("{v:x}").as_str())
        );
    }
}

#[test]
fn a_rejected_version_still_produces_a_parseable_record() {
    // `ver=` with nothing after it would break the `key=value` split on the way
    // back in and take the whole crash with it — so the rejection has to write a
    // WORD, not an empty string. The crash is the valuable half of the record;
    // it must not be lost because the version was odd.
    let crash = NativeCrash {
        signal: NativeSignal::Other,
        code: 0,
        fault: FaultAddress::Unknown,
        site: Site::Unknown,
        thread: ThreadRole::Unknown,
    };
    for hostile in ["//////", "", "   ", "\n"] {
        let line = line_of(&crash, hostile);
        assert!(line.contains(" ver=unknown"), "{line:?}");
        let (back, version) = NativeCrash::parse_line(&line).expect("parsed");
        assert_eq!(back, crash);
        assert_eq!(version, "unknown");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//   The two pure classifications
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn a_program_counter_is_ours_only_inside_our_own_image() {
    let (base, end) = (0x1000usize, 0x2000usize);
    assert_eq!(site_for_pc(0x1000, base, end), Site::App { offset: 0 });
    assert_eq!(site_for_pc(0x1234, base, end), Site::App { offset: 0x234 });
    // Half-open: the byte after the image is not in the image.
    assert_eq!(site_for_pc(0x1fff, base, end), Site::App { offset: 0xfff });
    assert_eq!(site_for_pc(0x2000, base, end), Site::Foreign);
    assert_eq!(site_for_pc(0x0fff, base, end), Site::Foreign);
    assert_eq!(site_for_pc(u64::MAX, base, end), Site::Foreign);
    // An unresolved range never invents an offset — the failure mode that would
    // publish a number meaning nothing at all.
    assert_eq!(site_for_pc(0x1234, 0, 0), Site::Unknown);
    assert_eq!(site_for_pc(0x1234, 0x2000, 0x1000), Site::Unknown);
    // A null program counter is a jump through a null function pointer; there is
    // no offset to report and pretending otherwise would say "app+0x0".
    assert_eq!(site_for_pc(0, base, end), Site::Unknown);
}

#[test]
fn a_thread_is_only_the_main_one_when_both_ids_are_known_and_equal() {
    assert_eq!(thread_role(7, 7), ThreadRole::Main);
    assert_eq!(thread_role(8, 7), ThreadRole::Other);
    // Zero means "the platform would not say" on either side — never a guess.
    assert_eq!(thread_role(0, 7), ThreadRole::Unknown);
    assert_eq!(thread_role(7, 0), ThreadRole::Unknown);
    assert_eq!(thread_role(0, 0), ThreadRole::Unknown);
}

#[test]
fn a_faulting_address_survives_only_as_four_values() {
    assert_eq!(FaultAddress::classify(0), FaultAddress::Null);
    assert_eq!(FaultAddress::classify(1), FaultAddress::LowPage);
    assert_eq!(FaultAddress::classify(4095), FaultAddress::LowPage);
    assert_eq!(FaultAddress::classify(4096), FaultAddress::NonNull);
    assert_eq!(FaultAddress::classify(u64::MAX), FaultAddress::NonNull);
    // The value itself never appears in the wire spelling — the whole point.
    for addr in [0u64, 1, 4095, 4096, 0x7fff_1234_5678, u64::MAX] {
        assert!(!FaultAddress::classify(addr)
            .as_str()
            .contains(char::is_numeric));
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//   Adoption, against a real filesystem
// ─────────────────────────────────────────────────────────────────────────────

fn write_pending_file(data_dir: &Path, body: &str) {
    std::fs::write(pending_path(data_dir), body).expect("write pending");
}

#[test]
fn adoption_turns_a_pending_record_into_an_ordinary_ring_entry() {
    let dir = tempfile::tempdir().expect("tempdir");
    let crash = NativeCrash {
        signal: NativeSignal::Segv,
        code: 1,
        fault: FaultAddress::Null,
        site: Site::App { offset: 0x1a2b3c },
        thread: ThreadRole::Main,
    };
    write_pending_file(dir.path(), &line_of(&crash, "0.6.0"));

    assert!(adopt(dir.path()), "a pending record must be adopted");

    let ring = crash_ring::read_entries(&crash_ring::crash_dir(dir.path()));
    assert_eq!(ring.len(), 1);
    let e = &ring[0];
    assert_eq!(e.kind, CrashKind::Other);
    assert_eq!(e.message, "native crash: sig=segv code=1 fault=null");
    assert_eq!(e.location.as_deref(), Some("app+0x1a2b3c"));
    assert_eq!(e.task.as_deref(), Some("main-thread"));
    // The version that CRASHED, not the version adopting — the update that
    // followed the crash must not rewrite history.
    assert_eq!(e.app_version, "0.6.0");
    assert_ne!(e.app_version, crate::telemetry::app_version());
    // The timestamp is the file's, i.e. when the handler wrote it.
    assert!(e.at > 1_700_000_000_000, "{}", e.at);

    // The file is emptied, so the next launch does not adopt it again.
    assert!(!adopt(dir.path()), "a record must be adopted exactly once");
    assert_eq!(
        crash_ring::count(&crash_ring::crash_dir(dir.path())),
        1,
        "adoption must not duplicate the entry"
    );
}

#[test]
fn adoption_is_a_no_op_after_a_clean_run() {
    // The overwhelmingly common case: the app exited normally, so the file is
    // the empty one `arm` truncated it to. It must produce no record, no log
    // noise and no ring entry.
    let dir = tempfile::tempdir().expect("tempdir");
    assert!(!adopt(dir.path()), "no file at all");
    write_pending_file(dir.path(), "");
    assert!(!adopt(dir.path()), "an empty file");
    write_pending_file(dir.path(), "\n  \n");
    assert!(!adopt(dir.path()), "whitespace");
    assert_eq!(crash_ring::count(&crash_ring::crash_dir(dir.path())), 0);
}

#[test]
fn an_unreadable_record_is_discarded_once_and_not_re_read_every_launch() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_pending_file(dir.path(), "sundaystage-native-crash v1 sig=lyrics\n");
    assert!(!adopt(dir.path()));
    assert_eq!(crash_ring::count(&crash_ring::crash_dir(dir.path())), 0);
    // Cleared anyway: a file we cannot use must not be re-parsed, and re-logged,
    // on every launch for the rest of this install's life.
    assert_eq!(
        std::fs::read_to_string(pending_path(dir.path())).expect("read"),
        ""
    );
}

// ─────────────────────────────────────────────────────────────────────────────
//   The operator's switch
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn hard_crash_capture_is_on_unless_the_operator_turned_it_off() {
    let dir = tempfile::tempdir().expect("tempdir");
    // No file at all: on. This is a local diagnostic, not a transmission.
    assert!(capture_enabled(dir.path()));

    set_capture_enabled(dir.path(), false).expect("write");
    assert!(!capture_enabled(dir.path()));
    // …and `arm` honours it without touching any process-global state.
    assert_eq!(arm(dir.path()), ArmOutcome::Disabled);

    set_capture_enabled(dir.path(), true).expect("write");
    assert!(capture_enabled(dir.path()));

    // A garbled file reads as ON — the opposite of the consent record's rule,
    // and deliberately so: nothing here decides whether anything is SENT.
    std::fs::write(settings_path(dir.path()), "{ not json").expect("write");
    assert!(capture_enabled(dir.path()));
    std::fs::write(settings_path(dir.path()), "{\"enabled\":\"maybe\"}").expect("write");
    assert!(capture_enabled(dir.path()));
}

// ─────────────────────────────────────────────────────────────────────────────
//   The consent gate, through the real drain
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_hard_crash_captured_before_consent_is_never_sent() {
    use crate::db::repositories::TelemetryRepo;
    use crate::db::Database;
    use crate::telemetry::client;

    let db = Database::open_in_memory().await.expect("db");
    let dir = tempfile::tempdir().expect("tempdir");
    let ring = crash_ring::crash_dir(dir.path());

    // A hard crash on a machine that has never been asked about telemetry —
    // which is the state EVERY install is in until the operator answers.
    let crash = NativeCrash {
        signal: NativeSignal::Segv,
        code: 1,
        fault: FaultAddress::Null,
        site: Site::App { offset: 0x40 },
        thread: ThreadRole::Main,
    };
    write_pending_file(dir.path(), &line_of(&crash, "0.7.0"));
    assert!(adopt(dir.path()));
    assert_eq!(crash_ring::count(&ring), 1, "captured locally");

    // Gate 1: no consent, no drain at all.
    assert!(
        !client::drain(&db.pool, dir.path(), "0.7.0")
            .await
            .expect("drain"),
        "a machine that was never asked must queue nothing"
    );

    // Gate 2: the operator now says yes. Granting starts reporting FROM NOW —
    // the archive of crashes collected while nobody had said anything is not
    // retroactively consented to.
    client::consent_set(&db.pool, true).await.expect("grant");
    assert!(
        !client::drain(&db.pool, dir.path(), "0.7.0")
            .await
            .expect("drain"),
        "the pre-consent crash is behind the watermark and must stay on the machine"
    );
    assert_eq!(
        TelemetryRepo::new(&db.pool)
            .outbox_load()
            .await
            .expect("outbox")
            .len(),
        0
    );
    assert_eq!(
        crash_ring::count(&ring),
        1,
        "and it is still there locally — it just never goes anywhere"
    );

    // Gate 3: a crash AFTER the answer does travel, and arrives intact.
    let watermark = client::crash_watermark(&db.pool).await.expect("watermark");
    crash_ring::write_record(&ring, &crash.to_entry("0.7.0", watermark + 1)).expect("write");
    assert!(
        client::drain(&db.pool, dir.path(), "0.7.0")
            .await
            .expect("drain"),
        "a crash the operator has consented to must be queued"
    );

    let queued = TelemetryRepo::new(&db.pool)
        .outbox_load()
        .await
        .expect("outbox");
    assert_eq!(queued.len(), 1);
    let body = &queued[0].payload_json;
    // Exactly one crash on the wire: the pre-consent one is not smuggled along
    // with the post-consent one.
    let parsed: serde_json::Value = serde_json::from_str(body).expect("json");
    let crashes = parsed["crashes"].as_array().expect("crashes");
    assert_eq!(crashes.len(), 1, "{body}");
    assert_eq!(crashes[0]["kind"], "other");
    assert_eq!(
        crashes[0]["message"],
        "native crash: sig=segv code=1 fault=null"
    );
    assert_eq!(crashes[0]["location"], "app+0x40");
    assert_eq!(crashes[0]["task"], "main-thread");
    assert_eq!(crashes[0]["backtracePresent"], false);

    // And the whole serialised payload is still free of the shapes law 2 names.
    for needle in ["/Users", "C:\\", "\\\\", "~/"] {
        assert!(!body.contains(needle), "{needle} reached the wire: {body}");
    }
}

#[tokio::test]
async fn revoking_consent_stops_a_hard_crash_from_travelling() {
    use crate::db::repositories::TelemetryRepo;
    use crate::db::Database;
    use crate::telemetry::client;

    let db = Database::open_in_memory().await.expect("db");
    let dir = tempfile::tempdir().expect("tempdir");
    let ring = crash_ring::crash_dir(dir.path());

    client::consent_set(&db.pool, true).await.expect("grant");
    let watermark = client::crash_watermark(&db.pool).await.expect("watermark");
    let crash = NativeCrash {
        signal: NativeSignal::Abort,
        code: 0,
        fault: FaultAddress::Unknown,
        site: Site::Foreign,
        thread: ThreadRole::Other,
    };
    crash_ring::write_record(&ring, &crash.to_entry("0.7.0", watermark + 1)).expect("write");

    // The operator changes their mind before the queue drains.
    client::consent_set(&db.pool, false).await.expect("revoke");
    assert!(
        !client::drain(&db.pool, dir.path(), "0.7.0")
            .await
            .expect("drain"),
        "off must mean nothing is queued"
    );
    assert_eq!(
        TelemetryRepo::new(&db.pool)
            .outbox_load()
            .await
            .expect("outbox")
            .len(),
        0
    );
}

// ─────────────────────────────────────────────────────────────────────────────
//   The signal-handler discipline, as a property of the source
// ─────────────────────────────────────────────────────────────────────────────

/// Every source file that contains code reachable from the crash callback.
///
/// Included as TEXT, not compiled: the three platform halves are checked on
/// every platform, so a Windows-only edit that starts allocating is caught by a
/// pull request whose CI only runs `cargo test` on Linux.
const HANDLER_SOURCES: [(&str, &str); 4] = [
    ("native_crash.rs", include_str!("../native_crash.rs")),
    (
        "native_crash_macos.rs",
        include_str!("../native_crash_macos.rs"),
    ),
    (
        "native_crash_linux.rs",
        include_str!("../native_crash_linux.rs"),
    ),
    (
        "native_crash_windows.rs",
        include_str!("../native_crash_windows.rs"),
    ),
];

#[test]
fn nothing_on_the_handler_path_allocates_locks_or_panics() {
    // The rule this module lives or dies by, enforced against the SOURCE rather
    // than against a belief. Everything between `HANDLER-SAFE-BEGIN` and
    // `HANDLER-SAFE-END` can run inside a signal handler, where `malloc` may be
    // held by the thread that just died, a lock may never be released, and a
    // panic aborts the process outright. None of those failures show up in a
    // test that calls these functions from a healthy thread — which is exactly
    // why this test reads the text instead of running the code.
    //
    // If a legitimate change needs one of these, the marker moves; it does not
    // get an exception. There is no work worth doing inside a crash handler that
    // could not be done at arm time or at adoption time instead.
    let forbidden = [
        // Allocation.
        "format!",
        ".to_string()",
        "String::",
        "vec!",
        "Vec::",
        ".to_owned()",
        ".collect()",
        "Box::new",
        ".clone()",
        // Locking.
        ".lock()",
        ".read()",
        ".write()",
        // Panicking. (`unwrap_or`/`unwrap_or_default` are fine and deliberately
        // not matched — they are the non-panicking forms.)
        ".unwrap()",
        ".expect(",
        "panic!",
        "unreachable!",
        "todo!",
        "assert",
        // Ordinary I/O and logging, both of which allocate and take locks.
        "tracing::",
        "println!",
        "eprintln!",
    ];

    let mut regions = 0;
    for (name, source) in HANDLER_SOURCES {
        let mut rest = source;
        while let Some(start) = rest.find("HANDLER-SAFE-BEGIN") {
            let after = &rest[start..];
            let end = after
                .find("HANDLER-SAFE-END")
                .unwrap_or_else(|| panic!("{name}: an unterminated HANDLER-SAFE region"));
            // Comments are stripped first. They are where the rules are
            // EXPLAINED — "no `format!` here" — and a test that fails on its own
            // documentation trains people to weaken the test.
            let region: String = after[..end]
                .lines()
                .filter(|l| !l.trim_start().starts_with("//"))
                .collect::<Vec<_>>()
                .join("\n");
            for needle in forbidden {
                assert!(
                    !region.contains(needle),
                    "{name}: {needle} appears on the crash-handler path:\n{region}"
                );
            }
            regions += 1;
            rest = &after[end..];
        }
    }

    // The markers themselves have to still be there. A refactor that deleted
    // them would leave this test passing while checking nothing at all — the
    // failure mode of every source-reading test.
    assert_eq!(
        regions, 7,
        "expected 7 handler-safe regions (4 in native_crash.rs, 1 per platform); \
         found {regions} — did a marker get lost in a refactor?"
    );
}

#[test]
fn the_callback_always_hands_the_crash_back() {
    // The single most important line in the module, pinned as text because there
    // is no way to observe it from inside this process: a `Handled(true)` would
    // swallow the crash, and the app would quietly stop producing OS crash
    // reports, stop printing Rust's stack-overflow message, and start exiting
    // cleanly from faults. The child-process tests below prove the CONSEQUENCE;
    // this proves there is exactly one place it could go wrong.
    let source = HANDLER_SOURCES[0].1;
    assert_eq!(
        source.matches("CrashEventResult::Handled(true)").count(),
        0,
        "the callback must never claim to have handled a crash"
    );
    assert_eq!(
        source.matches("CrashEventResult::Handled(false)").count(),
        1,
        "`on_crash` has exactly one return, and it hands the crash back"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
//   A real hard crash, in a real process
// ─────────────────────────────────────────────────────────────────────────────

/// Environment variable naming the app-data directory a probe child should arm
/// into. Its presence is what turns [`native_crash_probe`] from a no-op into a
/// deliberately crashing process.
const PROBE_DIR_ENV: &str = "SUNDAYSTAGE_NATIVE_CRASH_PROBE_DIR";

/// Which way the probe should die.
const PROBE_KIND_ENV: &str = "SUNDAYSTAGE_NATIVE_CRASH_PROBE_KIND";

/// The probe. Ordinarily a no-op that passes; with [`PROBE_DIR_ENV`] set it arms
/// the real handler and then crashes for real.
///
/// This is not a test of its own — it is the child half of the two tests below,
/// run by re-executing this test binary. There is no other way to observe a
/// process dying of `SIGSEGV`: a crash that could be caught in-process would not
/// be the thing this module exists for.
#[test]
fn native_crash_probe() {
    let Ok(dir) = std::env::var(PROBE_DIR_ENV) else {
        return;
    };
    let kind = std::env::var(PROBE_KIND_ENV).unwrap_or_default();

    assert_eq!(
        arm(Path::new(&dir)),
        ArmOutcome::Armed,
        "the probe must actually arm"
    );

    match kind.as_str() {
        "abort" => std::process::abort(),
        _ => {
            // A write through a null pointer: the plainest possible SIGSEGV /
            // EXC_BAD_ACCESS / ACCESS_VIOLATION, and the one whose faulting
            // address is unambiguously null on every platform.
            //
            // SAFETY: there is none. That is the entire point of this line.
            unsafe { std::ptr::null_mut::<u8>().write_volatile(1) };
        }
    }
    unreachable!("the probe must not survive");
}

/// The probe's FULL test path. `--exact` matches the whole name, so a short one
/// silently selects zero tests and the child exits 0 — a green harness proving
/// nothing, which is the worst outcome available here.
const PROBE_TEST: &str = "telemetry::native_crash::tests::native_crash_probe";

/// Run the probe in a child process and return its exit status plus the app-data
/// directory it armed into.
fn run_probe(kind: &str) -> (std::process::ExitStatus, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let exe = std::env::current_exe().expect("this test binary");
    let status = std::process::Command::new(exe)
        .args([PROBE_TEST, "--exact", "--test-threads=1"])
        .env(PROBE_DIR_ENV, dir.path())
        .env(PROBE_KIND_ENV, kind)
        // The child is going to die of a signal; its output is the OS's business
        // and not something this test should mix into the harness's.
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .expect("spawn the probe")
        .status;
    assert!(
        !status.success(),
        "the probe exited cleanly, which means it never ran — check that {PROBE_TEST} \
         is still the test's full path"
    );
    (status, dir)
}

#[cfg(unix)]
#[test]
fn a_real_segfault_is_captured_and_still_kills_the_process() {
    use std::os::unix::process::ExitStatusExt as _;

    let (status, dir) = run_probe("segv");

    // Half one: we did not swallow the crash. The child still died OF THE
    // SIGNAL, which is what keeps the OS's own crash reporting, Rust's stack
    // overflow message and every outside watcher's view of the truth intact.
    // A handler that returned "handled" would leave a clean exit code here and
    // would be strictly worse than no handler at all.
    assert_eq!(
        status.signal(),
        Some(libc::SIGSEGV),
        "the child must still die of SIGSEGV, not exit cleanly ({status:?})"
    );
    assert_eq!(status.code(), None);

    // Half two: and we still learned what happened.
    assert!(adopt(dir.path()), "the crash must have left a record");
    let ring = crash_ring::read_entries(&crash_ring::crash_dir(dir.path()));
    assert_eq!(ring.len(), 1);
    let e = &ring[0];
    assert_eq!(e.kind, CrashKind::Other);
    assert!(
        e.message.starts_with("native crash: sig=segv "),
        "{}",
        e.message
    );
    assert!(
        e.message.ends_with("fault=null"),
        "a write through a null pointer must classify as null: {}",
        e.message
    );
    // The probe arms and crashes on the same thread, so the role has to resolve.
    assert_eq!(e.task.as_deref(), Some("main-thread"));
    // The crash is in this very binary, so the site must resolve to our own
    // image with an offset — the half of the signal that is worth having.
    let loc = e.location.clone().unwrap_or_default();
    assert!(
        loc.starts_with("app+0x"),
        "a fault in our own code must resolve inside our own image, got {loc:?}"
    );
    assert_ne!(loc, "app+0x0");
    assert_eq!(e.app_version, crate::telemetry::app_version());
    assert!(!e.backtrace_present);
}

#[cfg(unix)]
#[test]
fn a_real_abort_is_captured_as_an_abort() {
    use std::os::unix::process::ExitStatusExt as _;

    let (status, dir) = run_probe("abort");
    assert_eq!(
        status.signal(),
        Some(libc::SIGABRT),
        "the child must still die of SIGABRT ({status:?})"
    );

    assert!(adopt(dir.path()), "the abort must have left a record");
    let ring = crash_ring::read_entries(&crash_ring::crash_dir(dir.path()));
    assert_eq!(ring.len(), 1);
    assert!(
        ring[0].message.starts_with("native crash: sig=abort "),
        "{}",
        ring[0].message
    );
    // An abort has no faulting address, and inventing one would be a lie about
    // the only pointer-shaped thing in the record.
    assert!(
        ring[0].message.ends_with("fault=unknown"),
        "{}",
        ring[0].message
    );
}
