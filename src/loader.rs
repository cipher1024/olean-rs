use std::collections::hash_map::HashMap;
use std::collections::BTreeSet;
use std::collections::LinkedList;
use std::io;
use std::fs::File;
use crate::types::*;
use crate::deserialize;
use crate::leanpath::*;

pub struct Loader {
    pub map: HashMap<Name, (OLean, Option<Vec<Modification>>)>,
    pub order: Vec<Name>,
    pub lean_path: LeanPath
}

fn load_olean(lp: &LeanPath, n: Name) -> io::Result<Option<(Name, OLean)>> {
    Ok(if let Some((n2, file)) = lp.find(n.clone(), "olean") {
        Some((n2, deserialize::read_olean(File::open(file)?)?))
    } else { None })
}

impl Loader {
    pub fn new(lp: LeanPath) -> Loader { Loader { map: HashMap::new(), order: Vec::new(), lean_path: lp } }

    pub fn load(&mut self, start: Name) -> io::Result<()> {
        if let Some((n2, ol)) = load_olean(&self.lean_path, start.clone())? {
            for mp in &ol.imports {
                let other = mp.resolve(n2.clone());
                if !self.map.contains_key(&other) {
                    self.load(other)?
                }
            }
            self.order.push(n2.clone());
            self.map.insert(n2, (ol, None));
        } else { println!("can't find {}\npath = {:?}", start, self.lean_path.0) }
        Ok(())
    }

    pub fn get_mods(map: &mut HashMap<Name, (OLean, Option<Vec<Modification>>)>, n: Name) -> io::Result<&[Modification]> {
        let (ol, o) = map.get_mut(&n).expect("should already be loaded");
        if let Some(mods) = o { return Ok(mods) }
        let mods = deserialize::read_olean_modifications(&ol.code).map_err(|err| {
            io::Error::new(io::ErrorKind::InvalidData, format!("error parsing {}: {}", n, err))
        })?;
        Ok(o.get_or_insert(mods))
    }

    pub fn get_mods2(map: &mut HashMap<Name, (OLean, Option<Vec<Modification>>)>, n: Name) -> Option<&[Modification]> {
        let (ol, o) =
            if map.contains_key(&n) { map.get_mut(&n)? }
            else { map.get_mut(&n.clone().str("default".to_string()))? };
        if let Some(mods) = o { return Some(mods) }
        let mods = deserialize::read_olean_modifications(&ol.code).unwrap_or_else(|err|
            panic!("error parsing {}: {}", n, err)
        );
        return Some(o.get_or_insert(mods)) }

    fn decls(m : &[Modification]) -> Vec<&Declaration> {
        let mut arr: Vec<&Declaration> = Vec::new();
        for x in m {
            if let Modification::Decl{ decl: d, trust_lvl: _ } = &x {
                arr.push(&d) } }
        arr }

    fn type_decls(m : &[Modification]) -> Vec<&InductiveDefn> {
        let mut arr: Vec<&InductiveDefn> = Vec::new();
        for x in m {
            if let Modification::Inductive{ defn: d, trust_lvl: _ } = &x {
                arr.push(&d) } }
        arr }

    fn class_entry(m : &[Modification]) -> Vec<&ClassEntry> {
        let mut arr: Vec<&ClassEntry> = Vec::new();
        for x in m {
            if let Modification::Class(entry) = &x {
                arr.push(&entry) } }
        arr }

    fn attributes(m : &[Modification]) -> Vec<&AttrEntry> {
        let mut arr: Vec<&AttrEntry> = Vec::new();
        for x in m {
            if let Modification::Attr(entry) = &x {
                arr.push(&entry) } }
        arr }

    pub fn exported_syms(&mut self, n : &Name) -> Option<BTreeSet<Name>> {
        let mods = Loader::get_mods2(&mut self.map, n.clone()).expect("exported_syms");
        let decls = Loader::decls(mods);
        let type_decl = Loader::type_decls(mods);
        let attributes = Loader::attributes(mods);
        let class_entry = Loader::class_entry(mods);
        let set: BTreeSet<Name> =
            decls.iter().map(|d| d.name())
            .chain(type_decl.iter().map(|d| d.name()))
            .chain(attributes.iter().map(|d| d.name()))
            .chain(class_entry.iter().map(|d| d.name()))
            .collect();
        Some(set) }

    pub fn used_syms(&mut self, n : &Name) -> Option<BTreeSet<Name>> {
        let b = Loader::decls(Loader::get_mods2(&mut self.map, n.clone()).expect("used_syms"));
        let mut set: BTreeSet<Name> = BTreeSet::new();
        for d in b {
            d.ref_symbols_acc(&mut set)
        }
        Some(set) }

    pub fn iter_imports(&mut self, n : &Name) -> std::slice::Iter<ModuleName> {
        let msg = format!("unknown module 1 {:?}", n);
        let (ol,_) = self.map.get(&n).expect(msg.as_str());
        ol.imports.iter()
    }
    pub fn unused_imports(&mut self, n : &Name) -> Vec<Vec<Name>> {
        let msg = format!("unknown module 1 {:?}", n);
        let s = self.used_syms(n).expect(msg.as_str());
        let tactic : Name = name![tactic];
        if s.iter().any(|n| tactic.is_prefix_of(&n)) ||
            n.drop_prefix() == name![default]
        { return Vec::new() }
        let mut path = LinkedList::new();
        let mut result = Vec::new();
        // println!("*** used symbols");
        // for x in &s {  println!("- {:?}", x) }
        self.unused_imports_acc(n,&s,&mut path,&mut result);
        result }

    pub fn unused_imports_acc(&mut self, n : &Name, s : &BTreeSet<Name>, path : &mut LinkedList<Name>, result : &mut Vec<Vec<Name>>) {
        let tactic : Name = name![tactic];
        let msg = format!("unknown module 2 {:?}", n);
        let (ol,_) = self.map.get(&n).expect(msg.as_str());
        let n2 : Name = self.lean_path.find(n.clone(), "olean").expect(msg.as_str()).0;
        let imps : Vec<Name> = ol.imports.iter().map(|m| m.resolve(n2.clone())).collect();
        for m in imps {
            if m != name![init] {
                let syms : BTreeSet<Name> = self.exported_syms(&m).expect(format!("unknown module {:?}", m).as_str());
                path.push_front(m.clone());
                let def_name = m.clone().str("default".to_string());
                if m.drop_prefix() == name![default] {
                    self.unused_imports_acc(&m, s, path, result)
                } else if self.map.contains_key(&def_name) {
                    self.unused_imports_acc(&def_name, s, path, result)
                } else {
                    // println!("*** for module '{:?}'", m);
                    if syms.is_disjoint(&s) && !syms.iter().any(|n| tactic.is_prefix_of(&n)) {
                        // for x in &syms { println!("- {:?}", x) }
                        result.push(path.iter().cloned().collect())
                    }
                    // else { println!("- <skip>") }
                }
                path.pop_front(); }
        }
    }
}
