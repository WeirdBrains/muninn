//! A replicated text (RGA): every character has a unique id, inserts go after a known
//! character, deletes only hide. Any two copies that have applied the same operations,
//! in any order that keeps each insert after the character it follows, hold the same text,
//! so concurrent edits merge without losing anyone's characters.
use super::proto::{Delete, Id, Insert, Span};
use std::collections::HashSet;

#[derive(Debug, Clone)]
struct Elem {
    id: Id,
    ch: char,
    deleted: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Text {
    elems: Vec<Elem>,
    /// The highest counter seen (Lamport clock).
    clock: u64,
    /// Inserts applied (by their first id), so a repeat is a no-op.
    applied: HashSet<Id>,
    /// Deletes that arrived before their characters.
    pending: Vec<Id>,
}

fn after_of(id: Id, i: u64) -> Id {
    Id { site: id.site, counter: id.counter + i }
}

impl Text {
    pub fn new() -> Text {
        Text::default()
    }

    pub fn text(&self) -> String {
        self.elems.iter().filter(|e| !e.deleted).map(|e| e.ch).collect()
    }

    pub fn len(&self) -> usize {
        self.elems.iter().filter(|e| !e.deleted).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn clock(&self) -> u64 {
        self.clock
    }

    fn index(&self, id: Id) -> Option<usize> {
        self.elems.iter().position(|e| e.id == id)
    }

    /// Apply an insert (idempotent). False if the character it follows is unknown.
    pub fn insert(&mut self, op: &Insert) -> bool {
        let Some(first) = op.id else { return false };
        if self.applied.contains(&first) {
            return true;
        }
        let mut pos = match op.after {
            None => 0,
            Some(a) => match self.index(a) {
                Some(i) => i + 1,
                None => return false,
            },
        };
        for (i, ch) in op.text.chars().enumerate() {
            let id = after_of(first, i as u64);
            if i == 0 {
                // skip concurrent inserts at the same place that sort after this one
                while pos < self.elems.len() && self.elems[pos].id > id {
                    pos += 1;
                }
            }
            self.elems.insert(pos, Elem { id, ch, deleted: false });
            pos += 1;
            self.clock = self.clock.max(id.counter);
        }
        self.applied.insert(first);
        let pending = std::mem::take(&mut self.pending);
        for id in pending {
            if let Some(i) = self.index(id) {
                self.elems[i].deleted = true;
            } else {
                self.pending.push(id);
            }
        }
        true
    }

    pub fn delete(&mut self, op: &Delete) {
        for s in &op.spans {
            for k in 0..s.len {
                let id = Id { site: s.site, counter: s.start + k };
                match self.index(id) {
                    Some(i) => self.elems[i].deleted = true,
                    None => self.pending.push(id),
                }
            }
        }
    }

    /// The id of the n-th visible character.
    fn visible_id(&self, n: usize) -> Option<Id> {
        self.elems.iter().filter(|e| !e.deleted).nth(n).map(|e| e.id)
    }

    /// Insert `s` at visible offset `at` as `site`: the operation (already applied).
    pub fn local_insert(&mut self, site: u64, at: usize, s: &str) -> Option<Insert> {
        if s.is_empty() {
            return None;
        }
        let after = if at == 0 { None } else { self.visible_id(at - 1) };
        let op = Insert { id: Some(Id { site, counter: self.clock + 1 }), after, text: s.to_string() };
        self.insert(&op);
        Some(op)
    }

    /// Delete `n` visible characters from offset `at`: the operation (already applied).
    pub fn local_delete(&mut self, at: usize, n: usize) -> Option<Delete> {
        if n == 0 {
            return None;
        }
        let ids: Vec<Id> = self.elems.iter().filter(|e| !e.deleted).skip(at).take(n).map(|e| e.id).collect();
        let mut spans: Vec<Span> = vec![];
        for id in ids {
            match spans.last_mut() {
                Some(s) if s.site == id.site && s.start + s.len == id.counter => s.len += 1,
                _ => spans.push(Span { site: id.site, start: id.counter, len: 1 }),
            }
        }
        let op = Delete { spans };
        self.delete(&op);
        Some(op)
    }

    /// The operations that turn the current text into `new` (a single changed region),
    /// already applied.
    pub fn local_edit(&mut self, site: u64, new: &str) -> (Option<Delete>, Option<Insert>) {
        let old: Vec<char> = self.text().chars().collect();
        let nw: Vec<char> = new.chars().collect();
        let pre = old.iter().zip(&nw).take_while(|(a, b)| a == b).count();
        let max_suf = old.len().min(nw.len()) - pre;
        let suf = old.iter().rev().zip(nw.iter().rev()).take(max_suf).take_while(|(a, b)| a == b).count();
        let del = self.local_delete(pre, old.len() - pre - suf);
        let ins: String = nw[pre..nw.len() - suf].iter().collect();
        let ins = self.local_insert(site, pre, &ins);
        (del, ins)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concurrent_edits_converge_in_any_order() {
        let mut base = Text::new();
        base.local_insert(0, 0, "hello world").unwrap();
        // a and b edit at the same time without seeing each other
        let mut a = base.clone();
        let (_, ia1) = a.local_edit(1, "hello brave world");
        let (_, ia2) = a.local_edit(1, "hello brave new world");
        let mut b = base.clone();
        let (db, ib) = b.local_edit(2, "hello world!");
        assert!(db.is_none());
        let (ia1, ia2, ib) = (ia1.unwrap(), ia2.unwrap(), ib.unwrap());
        // each applies the other's operations
        a.insert(&ib);
        b.insert(&ia1);
        b.insert(&ia2);
        assert_eq!(a.text(), b.text());
        assert_eq!(a.text(), "hello brave new world!");
        // a fresh copy gets everything in yet another order
        let mut c = base.clone();
        c.insert(&ib);
        c.insert(&ia1);
        c.insert(&ia2);
        assert_eq!(c.text(), a.text());
        // a concurrent delete and insert in the same region keep the insert
        let mut d = c.clone();
        let mut e = c.clone();
        let del = d.local_edit(3, "hello world!").0.unwrap();
        let ins = e.local_edit(4, "hello brave new shiny world!").1.unwrap();
        d.insert(&ins);
        e.delete(&del);
        assert_eq!(d.text(), e.text());
        assert_eq!(d.text(), "hello shiny world!");
    }

    #[test]
    fn same_place_inserts_keep_both_and_deletes_wait() {
        let mut base = Text::new();
        base.local_insert(0, 0, "ac").unwrap();
        let (mut x, mut y) = (base.clone(), base.clone());
        let ix = x.local_insert(1, 1, "X").unwrap();
        let iy = y.local_insert(2, 1, "Y").unwrap();
        x.insert(&iy);
        y.insert(&ix);
        assert_eq!(x.text(), y.text());
        assert!(x.text().contains('X') && x.text().contains('Y'));
        // a delete of a character not seen yet waits for it
        let mut z = base.clone();
        let mut w = base.clone();
        w.insert(&ix);
        let d = w.local_delete(1, 1).unwrap();
        z.delete(&d);
        z.insert(&ix);
        assert_eq!(z.text(), w.text());
        assert_eq!(z.text(), "ac");
        // repeats are no-ops
        z.insert(&ix);
        assert_eq!(z.text(), "ac");
    }
}
