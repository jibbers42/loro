//! Differential harness: the same random text histories run through stock
//! `loro` 1.17.1 (crates.io) and through the fork (`../../crates/loro`), with
//! every observable result compared: text, rich text, version vectors,
//! frontiers, diffs, checkouts, forks, undo/redo results and exported bytes.
//!
//! A "world" is a set of peers; each peer is backed by one of the two
//! libraries. Worlds with different assignments (all stock, all fork, mixed)
//! must produce identical traces, which also proves the two libraries read
//! each other's updates and snapshots.

use std::fmt::Write as _;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lib {
    Stock,
    Fork,
}

#[derive(Clone, Copy, Debug)]
pub enum ExportKind {
    UpdatesSince,
    AllUpdates,
    Snapshot,
    ShallowSnapshot,
}

pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03)
    }

    pub fn next(&mut self) -> u64 {
        // splitmix64
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A value in `0..n` (`n` > 0).
    pub fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    pub fn range(&mut self, lo: usize, hi: usize) -> usize {
        lo + self.below(hi - lo + 1)
    }
}

pub fn hash(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01B3);
    }
    h
}

fn digest(bytes: &[u8]) -> String {
    format!("{}b#{:016x}", bytes.len(), hash(bytes))
}

fn gen_text(rng: &mut Rng, chars: usize) -> String {
    const WORDS: &[&str] = &[
        "loro", "note", "sync", "the", "a", "markdown", "edit", "peer", "text", "of", "and",
        "**bold**", "`code`", "\n", "\n\n", "é", "漢字", "😀", "- item", "# title",
    ];
    let mut s = String::new();
    let mut n = 0;
    while n < chars {
        let w = WORDS[rng.below(WORDS.len())];
        for c in w.chars().chain(std::iter::once(' ')) {
            if n == chars {
                break;
            }
            s.push(c);
            n += 1;
        }
    }
    s
}

macro_rules! peer_impl {
    ($m:ident, $k:ident) => {
        pub mod $m {
            use super::{digest, ExportKind};
            use std::fmt::Write as _;
            use std::sync::{Arc, Mutex};
            use $k::{ExportMode, Frontiers, LoroDoc, LoroText, UndoManager, VersionVector};

            pub struct Peer {
                pub doc: LoroDoc,
                text: LoroText,
                undo: UndoManager,
                versions: Vec<Frontiers>,
                outbox: Arc<Mutex<Vec<Vec<u8>>>>,
                /// Count and running hash of every change event delivered.
                events: Arc<Mutex<(u64, u64)>>,
                _sub: $k::Subscription,
                _event_sub: $k::Subscription,
            }

            fn fmt_frontiers(f: &Frontiers) -> String {
                let mut ids: Vec<(u64, i32)> = f.iter().map(|id| (id.peer, id.counter)).collect();
                ids.sort();
                format!("{ids:?}")
            }

            fn fmt_vv(vv: &VersionVector) -> String {
                let mut v: Vec<(u64, i32)> = vv.iter().map(|(p, c)| (*p, *c)).collect();
                v.sort();
                format!("{v:?}")
            }

            fn fmt_text(doc: &LoroDoc) -> String {
                let text = doc.get_text("note");
                let plain = text.to_string();
                let rich = serde_json::to_value(text.get_richtext_value()).unwrap();
                format!(
                    "text[{}c {}] rich[{}]",
                    text.len_unicode(),
                    digest(plain.as_bytes()),
                    digest(rich.to_string().as_bytes())
                )
            }

            impl Peer {
                pub fn new(id: u64) -> Self {
                    let doc = LoroDoc::new();
                    doc.set_peer_id(id).unwrap();
                    let mut undo = UndoManager::new(&doc);
                    undo.set_merge_interval(0);
                    let outbox = Arc::new(Mutex::new(Vec::new()));
                    let sink = outbox.clone();
                    let sub = doc.subscribe_local_update(Box::new(move |b| {
                        sink.lock().unwrap().push(b.to_vec());
                        true
                    }));
                    let events = Arc::new(Mutex::new((0u64, 0u64)));
                    let seen = events.clone();
                    let event_sub = doc.subscribe_root(Arc::new(move |e| {
                        let mut seen = seen.lock().unwrap();
                        seen.0 += 1;
                        let described =
                            format!("{:?} {:?} {:?}", e.triggered_by, e.origin, e.events);
                        seen.1 = seen.1.rotate_left(7) ^ super::hash(described.as_bytes());
                    }));
                    let text = doc.get_text("note");
                    Peer {
                        versions: vec![doc.oplog_frontiers()],
                        doc,
                        text,
                        undo,
                        outbox,
                        events,
                        _sub: sub,
                        _event_sub: event_sub,
                    }
                }

                pub fn len(&self) -> usize {
                    self.text.len_unicode()
                }

                pub fn plain(&self) -> String {
                    self.text.to_string()
                }

                pub fn insert(&mut self, pos: usize, s: &str) -> String {
                    format!("{:?}", self.text.insert(pos, s))
                }

                pub fn delete(&mut self, pos: usize, len: usize) -> String {
                    format!("{:?}", self.text.delete(pos, len))
                }

                pub fn mark(&mut self, start: usize, end: usize, key: &str, on: bool) -> String {
                    if on {
                        format!("{:?}", self.text.mark(start..end, key, true))
                    } else {
                        format!("{:?}", self.text.unmark(start..end, key))
                    }
                }

                pub fn commit(&mut self) {
                    self.doc.commit();
                    let f = self.doc.oplog_frontiers();
                    if self.versions.last() != Some(&f) {
                        self.versions.push(f);
                    }
                }

                pub fn undo(&mut self) -> String {
                    let r = format!("{:?}", self.undo.undo());
                    self.commit();
                    r
                }

                pub fn redo(&mut self) -> String {
                    let r = format!("{:?}", self.undo.redo());
                    self.commit();
                    r
                }

                pub fn undo_counts(&self) -> (usize, usize) {
                    (self.undo.undo_count(), self.undo.redo_count())
                }

                pub fn vv(&self) -> Vec<u8> {
                    self.doc.oplog_vv().encode()
                }

                pub fn export(&self, kind: ExportKind, since: &[u8]) -> Vec<u8> {
                    match kind {
                        ExportKind::UpdatesSince => {
                            let vv = VersionVector::decode(since).unwrap();
                            self.doc.export(ExportMode::updates(&vv)).unwrap()
                        }
                        ExportKind::AllUpdates => {
                            self.doc.export(ExportMode::all_updates()).unwrap()
                        }
                        ExportKind::Snapshot => self.doc.export(ExportMode::Snapshot).unwrap(),
                        ExportKind::ShallowSnapshot => self
                            .doc
                            .export(ExportMode::shallow_snapshot(&self.doc.oplog_frontiers()))
                            .unwrap(),
                    }
                }

                /// Local-update blobs from index `from` on (what the app relays).
                pub fn outbox_from(&self, from: usize) -> Vec<Vec<u8>> {
                    self.outbox.lock().unwrap()[from..].to_vec()
                }

                pub fn outbox_len(&self) -> usize {
                    self.outbox.lock().unwrap().len()
                }

                pub fn import(&mut self, bytes: &[u8]) -> String {
                    let r = match self.doc.import(bytes) {
                        Ok(_) => "ok".to_string(),
                        Err(e) => format!("err {e:?}"),
                    };
                    self.commit();
                    r
                }

                pub fn import_batch(&mut self, blobs: &[Vec<u8>]) -> String {
                    let r = match self.doc.import_batch(blobs) {
                        Ok(_) => "ok".to_string(),
                        Err(e) => format!("err {e:?}"),
                    };
                    self.commit();
                    r
                }

                pub fn observe(&self) -> String {
                    let (event_count, event_hash) = *self.events.lock().unwrap();
                    format!(
                        "{} vv{} of{} sf{} undo{:?} events[{event_count} #{event_hash:016x}]",
                        fmt_text(&self.doc),
                        fmt_vv(&self.doc.oplog_vv()),
                        fmt_frontiers(&self.doc.oplog_frontiers()),
                        fmt_frontiers(&self.doc.state_frontiers()),
                        self.undo_counts()
                    )
                }

                pub fn version_count(&self) -> usize {
                    self.versions.len()
                }

                /// Diff both ways between two remembered versions, check out
                /// each, fork at each, and return to the latest version.
                pub fn time_travel(&mut self, i: usize, j: usize) -> String {
                    let a = self.versions[i].clone();
                    let b = self.versions[j].clone();
                    let mut out = String::new();
                    for (x, y) in [(&a, &b), (&b, &a)] {
                        match self.doc.diff(x, y) {
                            Ok(d) => write!(out, "diff[{}] ", digest(format!("{d:?}").as_bytes())),
                            Err(e) => write!(out, "diff[err {e:?}] "),
                        }
                        .unwrap();
                    }
                    for v in [&a, &b] {
                        match self.doc.checkout(v) {
                            Ok(()) => write!(
                                out,
                                "co[{} sf{}] ",
                                fmt_text(&self.doc),
                                fmt_frontiers(&self.doc.state_frontiers())
                            ),
                            Err(e) => write!(out, "co[err {e:?}] "),
                        }
                        .unwrap();
                    }
                    self.doc.checkout_to_latest();
                    write!(out, "latest[{}] ", fmt_text(&self.doc)).unwrap();
                    for v in [&a, &b] {
                        match self.doc.fork_at(v) {
                            Ok(f) => write!(
                                out,
                                "fork[{} {}] ",
                                fmt_text(&f),
                                digest(&f.export(ExportMode::Snapshot).unwrap())
                            ),
                            Err(e) => write!(out, "fork[err {e:?}] "),
                        }
                        .unwrap();
                    }
                    out
                }

                /// `revert_to` a remembered version (a diff applied as new ops).
                pub fn revert(&mut self, i: usize) -> String {
                    let v = self.versions[i].clone();
                    let r = format!("{:?}", self.doc.revert_to(&v));
                    self.commit();
                    r
                }
            }

            /// A new doc that imports `bytes`, described.
            pub fn fresh_from(bytes: &[u8]) -> String {
                let doc = LoroDoc::new();
                doc.set_peer_id(999).unwrap();
                match doc.import(bytes) {
                    Ok(_) => format!("{} vv{}", fmt_text(&doc), fmt_vv(&doc.oplog_vv())),
                    Err(e) => format!("err {e:?}"),
                }
            }
        }
    };
}

peer_impl!(fork, loro_fork);
peer_impl!(stock, loro_stock);

pub enum Peer {
    Stock(stock::Peer),
    Fork(fork::Peer),
}

macro_rules! each {
    ($self:expr, $p:ident => $e:expr) => {
        match $self {
            Peer::Stock($p) => $e,
            Peer::Fork($p) => $e,
        }
    };
}

impl Peer {
    pub fn new(lib: Lib, id: u64) -> Self {
        match lib {
            Lib::Stock => Peer::Stock(stock::Peer::new(id)),
            Lib::Fork => Peer::Fork(fork::Peer::new(id)),
        }
    }
    pub fn lib(&self) -> Lib {
        match self {
            Peer::Stock(_) => Lib::Stock,
            Peer::Fork(_) => Lib::Fork,
        }
    }
    pub fn len(&self) -> usize {
        each!(self, p => p.len())
    }
    pub fn plain(&self) -> String {
        each!(self, p => p.plain())
    }
    pub fn insert(&mut self, pos: usize, s: &str) -> String {
        each!(self, p => p.insert(pos, s))
    }
    pub fn delete(&mut self, pos: usize, len: usize) -> String {
        each!(self, p => p.delete(pos, len))
    }
    pub fn mark(&mut self, start: usize, end: usize, key: &str, on: bool) -> String {
        each!(self, p => p.mark(start, end, key, on))
    }
    pub fn commit(&mut self) {
        each!(self, p => p.commit())
    }
    pub fn undo(&mut self) -> String {
        each!(self, p => p.undo())
    }
    pub fn redo(&mut self) -> String {
        each!(self, p => p.redo())
    }
    pub fn vv(&self) -> Vec<u8> {
        each!(self, p => p.vv())
    }
    pub fn export(&self, kind: ExportKind, since: &[u8]) -> Vec<u8> {
        each!(self, p => p.export(kind, since))
    }
    pub fn outbox_from(&self, from: usize) -> Vec<Vec<u8>> {
        each!(self, p => p.outbox_from(from))
    }
    pub fn outbox_len(&self) -> usize {
        each!(self, p => p.outbox_len())
    }
    pub fn import(&mut self, bytes: &[u8]) -> String {
        each!(self, p => p.import(bytes))
    }
    pub fn import_batch(&mut self, blobs: &[Vec<u8>]) -> String {
        each!(self, p => p.import_batch(blobs))
    }
    pub fn observe(&self) -> String {
        each!(self, p => p.observe())
    }
    pub fn version_count(&self) -> usize {
        each!(self, p => p.version_count())
    }
    pub fn time_travel(&mut self, i: usize, j: usize) -> String {
        each!(self, p => p.time_travel(i, j))
    }
    pub fn revert(&mut self, i: usize) -> String {
        each!(self, p => p.revert(i))
    }
    pub fn fresh_from(&self, bytes: &[u8]) -> String {
        match self {
            Peer::Stock(_) => stock::fresh_from(bytes),
            Peer::Fork(_) => fork::fresh_from(bytes),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub peers: usize,
    pub steps: usize,
    /// Upper bound for a "large" insert, in characters. Anything above 256
    /// spans several id-to-cursor fragments, which is the fixed code path.
    pub large_insert: usize,
    /// Chance in percent that a large insert is `huge_insert` characters.
    pub huge_percent: usize,
    pub huge_insert: usize,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            peers: 3,
            steps: 120,
            large_insert: 3000,
            huge_percent: 4,
            huge_insert: 40_000,
        }
    }
}

/// Runs one random history. `assign[i]` is the library behind peer `i`; the
/// library never influences the random choices, so two assignments give the
/// same trace exactly when the libraries behave the same.
pub fn run_world(seed: u64, cfg: Config, assign: &[Lib]) -> Vec<String> {
    assert_eq!(assign.len(), cfg.peers);
    let mut rng = Rng::new(seed);
    let mut peers: Vec<Peer> = assign
        .iter()
        .enumerate()
        .map(|(i, &lib)| Peer::new(lib, i as u64 + 1))
        .collect();
    // delivered[to][from]: how many of `from`'s local-update blobs `to` has.
    let mut delivered = vec![vec![0usize; cfg.peers]; cfg.peers];
    let mut cursor = vec![0usize; cfg.peers];
    let mut trace = Vec::new();

    for step in 0..cfg.steps {
        let p = rng.below(cfg.peers);
        let len = peers[p].len();
        let roll = rng.below(100);
        let mut line = format!("{step} p{p} ");
        match roll {
            0..=29 => {
                // Typing: mostly continues where this peer last typed.
                let pos = if rng.below(4) == 0 {
                    rng.below(len + 1)
                } else {
                    cursor[p].min(len)
                };
                let n = rng.range(1, 12);
                let s = gen_text(&mut rng, n);
                let r = peers[p].insert(pos, &s);
                cursor[p] = pos + n;
                write!(line, "type@{pos}+{n} {r}").unwrap();
            }
            30..=41 => {
                let n = if rng.below(100) < cfg.huge_percent {
                    cfg.huge_insert
                } else {
                    rng.range(200, cfg.large_insert)
                };
                let pos = rng.below(len + 1);
                let s = gen_text(&mut rng, n);
                let r = peers[p].insert(pos, &s);
                cursor[p] = pos + n;
                write!(line, "paste@{pos}+{n} {r}").unwrap();
            }
            42..=52 if len > 0 => {
                let pos = rng.below(len);
                let n = rng.range(1, 5).min(len - pos);
                let r = peers[p].delete(pos, n);
                cursor[p] = pos;
                write!(line, "del@{pos}-{n} {r}").unwrap();
            }
            53..=58 if len > 0 => {
                let pos = rng.below(len);
                let n = rng.range(1, (len / 2).clamp(1, 6000)).min(len - pos);
                let r = peers[p].delete(pos, n);
                cursor[p] = pos;
                write!(line, "cut@{pos}-{n} {r}").unwrap();
            }
            59..=64 if len > 0 => {
                // Backspace run: single deletes going left, in one commit.
                let end = rng.range(1, len);
                let n = rng.range(2, 600).min(end);
                for i in 0..n {
                    peers[p].delete(end - 1 - i, 1);
                }
                cursor[p] = end - n;
                write!(line, "backspace@{end}-{n}").unwrap();
            }
            65..=69 if len > 1 => {
                let start = rng.below(len - 1);
                let span = rng.below(2000);
                let end = rng.range(start + 1, (start + 1 + span).min(len));
                let key = ["bold", "italic", "link"][rng.below(3)];
                let on = rng.below(4) != 0;
                let r = peers[p].mark(start, end, key, on);
                write!(line, "mark@{start}..{end} {key} {on} {r}").unwrap();
            }
            70..=77 => {
                let r = peers[p].undo();
                write!(line, "undo {r}").unwrap();
                // A redo only works straight after an undo; sometimes with a
                // remote edit arriving in between.
                match rng.below(4) {
                    0 => {
                        let r = peers[p].redo();
                        write!(line, " redo {r}").unwrap();
                    }
                    1 if cfg.peers > 1 => {
                        let q = (p + 1) % cfg.peers;
                        let bytes = peers[q].export(ExportKind::UpdatesSince, &peers[p].vv());
                        let i = peers[p].import(&bytes);
                        let r = peers[p].redo();
                        write!(line, " import<-p{q} {i} redo {r}").unwrap();
                    }
                    _ => {}
                }
            }
            78..=81 => {
                let r = peers[p].redo();
                write!(line, "redo {r}").unwrap();
            }
            82..=92 if cfg.peers > 1 => {
                let q = (p + 1 + rng.below(cfg.peers - 1)) % cfg.peers;
                match rng.below(5) {
                    0 => {
                        let blobs = peers[q].outbox_from(delivered[p][q]);
                        delivered[p][q] = peers[q].outbox_len();
                        write!(line, "relay<-p{q} x{}", blobs.len()).unwrap();
                        for b in &blobs {
                            let r = peers[p].import(b);
                            write!(line, " {} {r}", digest(b)).unwrap();
                        }
                    }
                    1 => {
                        let blobs = peers[q].outbox_from(delivered[p][q]);
                        delivered[p][q] = peers[q].outbox_len();
                        let r = peers[p].import_batch(&blobs);
                        let all: Vec<u8> = blobs.concat();
                        write!(line, "batch<-p{q} x{} {} {r}", blobs.len(), digest(&all)).unwrap();
                    }
                    2 => {
                        let bytes = peers[q].export(ExportKind::UpdatesSince, &peers[p].vv());
                        let r = peers[p].import(&bytes);
                        write!(line, "updates<-p{q} {} {r}", digest(&bytes)).unwrap();
                    }
                    3 => {
                        let bytes = peers[q].export(ExportKind::Snapshot, &[]);
                        let r = peers[p].import(&bytes);
                        write!(line, "snapshot<-p{q} {} {r}", digest(&bytes)).unwrap();
                    }
                    _ => {
                        // Everything every other peer has, as one batch.
                        let vv = peers[p].vv();
                        let blobs: Vec<Vec<u8>> = (0..cfg.peers)
                            .filter(|&o| o != p)
                            .map(|o| peers[o].export(ExportKind::UpdatesSince, &vv))
                            .collect();
                        let r = peers[p].import_batch(&blobs);
                        write!(line, "batch<-all {} {r}", digest(&blobs.concat())).unwrap();
                    }
                }
            }
            82..=97 => {
                let n = peers[p].version_count();
                let (i, j) = (rng.below(n), rng.below(n));
                let r = peers[p].time_travel(i, j);
                write!(line, "travel {i}->{j} {r}").unwrap();
            }
            98 => {
                let n = peers[p].version_count();
                let i = rng.below(n);
                let r = peers[p].revert(i);
                write!(line, "revert {i} {r}").unwrap();
            }
            _ => {
                let bytes = peers[p].export(ExportKind::ShallowSnapshot, &[]);
                let r = peers[p].fresh_from(&bytes);
                write!(line, "shallow {} {r}", digest(&bytes)).unwrap();
            }
        }
        // Mostly one action per commit; sometimes several share a commit.
        if rng.below(8) != 0 {
            peers[p].commit();
        }
        write!(line, " => {}", peers[p].observe()).unwrap();
        trace.push(line);
    }

    // Everyone syncs with everyone, twice, and must converge.
    for round in 0..2 {
        for to in 0..cfg.peers {
            peers[to].commit();
            for from in 0..cfg.peers {
                if from == to {
                    continue;
                }
                let bytes = if (round + to + from) % 2 == 0 {
                    peers[from].export(ExportKind::UpdatesSince, &peers[to].vv())
                } else {
                    peers[from].export(ExportKind::Snapshot, &[])
                };
                let r = peers[to].import(&bytes);
                trace.push(format!("final sync p{to}<-p{from} {} {r}", digest(&bytes)));
            }
        }
    }
    let first = peers[0].plain();
    for (i, peer) in peers.iter().enumerate() {
        assert_eq!(
            peer.plain(),
            first,
            "seed {seed}: peer {i} did not converge"
        );
        trace.push(format!("final p{i} {}", peer.observe()));
        for kind in [
            ExportKind::AllUpdates,
            ExportKind::Snapshot,
            ExportKind::ShallowSnapshot,
        ] {
            let bytes = peer.export(kind, &[]);
            // Each export is read back by both libraries.
            trace.push(format!(
                "final p{i} {kind:?} {} stock:{} fork:{}",
                digest(&bytes),
                stock::fresh_from(&bytes),
                fork::fresh_from(&bytes)
            ));
        }
    }
    for peer in peers.iter_mut() {
        // Undo what this peer still can (a checkout clears the undo stack,
        // so this comes before the time travel), then redo half of it.
        let mut undone = 0;
        while undone < 60 && peer.undo() == "Ok(true)" {
            undone += 1;
            if undone % 10 == 0 {
                trace.push(format!("final undo {undone} {}", peer.observe()));
            }
        }
        trace.push(format!("final undo x{undone} {}", peer.observe()));
        for _ in 0..undone / 2 {
            peer.redo();
        }
        trace.push(format!("final redo {}", peer.observe()));
        let n = peer.version_count();
        let r = peer.time_travel(0, n - 1);
        trace.push(format!("final travel {r}"));
        let r = peer.time_travel(n / 2, n - 1);
        trace.push(format!("final travel mid {r}"));
    }
    trace
}

pub fn assignments(peers: usize) -> Vec<(String, Vec<Lib>)> {
    let mut v = vec![
        ("all fork".to_string(), vec![Lib::Fork; peers]),
        (
            "fork/stock alternating".to_string(),
            (0..peers)
                .map(|i| if i % 2 == 0 { Lib::Fork } else { Lib::Stock })
                .collect(),
        ),
        (
            "stock/fork alternating".to_string(),
            (0..peers)
                .map(|i| if i % 2 == 0 { Lib::Stock } else { Lib::Fork })
                .collect(),
        ),
    ];
    v.truncate(if peers == 1 { 1 } else { 3 });
    v
}

/// Compares every assignment against the all-stock world. Returns a
/// description of the first difference, if any.
pub fn compare_seed(seed: u64, cfg: Config) -> Result<usize, String> {
    let base = run_world(seed, cfg, &vec![Lib::Stock; cfg.peers]);
    for (name, assign) in assignments(cfg.peers) {
        let other = run_world(seed, cfg, &assign);
        if other.len() != base.len() {
            return Err(format!(
                "seed {seed} [{name}]: trace length {} vs stock {}",
                other.len(),
                base.len()
            ));
        }
        for (i, (a, b)) in base.iter().zip(other.iter()).enumerate() {
            if a != b {
                return Err(format!(
                    "seed {seed} [{name}]: first difference at trace line {i}\n stock: {a}\n other: {b}"
                ));
            }
        }
    }
    Ok(base.len())
}
