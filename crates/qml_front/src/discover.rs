//! Where a project says what its modules are.
//!
//! `import Thermostat` names a module, and which files are its types is not
//! in any QML file: it is in the `qmldir` next to them, or in the
//! `CMakeLists.txt` that has the build write one. Both are read here for
//! that and nothing else.

use std::collections::HashMap;

/// A module, and the files that are its types.
#[derive(Debug, PartialEq, Eq)]
pub struct Module {
    pub uri: String,
    pub types: Vec<Type>,
    /// The files the build keeps in the program with the module
    /// (`RESOURCES`), from the directory of what describes it.
    pub resources: Vec<String>,
    /// Where in the program it keeps them: under `qrc:/qt/qml`, unless the
    /// build says otherwise (`RESOURCE_PREFIX`).
    pub prefix: String,
}

impl Module {
    /// `qrc:/qt/qml/Thermostat/images/icon.png`: what the program names one
    /// of its `resources` by.
    pub fn address(&self, resource: &str) -> String {
        address(&[&self.prefix, &self.uri.replace('.', "/"), resource])
    }
}

/// A file the build keeps in the program by itself (`qt_add_resources`).
#[derive(Debug, PartialEq, Eq)]
pub struct Resource {
    /// What the program names it by: `qrc:/data/medals.csv`.
    pub address: String,
    /// Where it is, from the directory of what describes it.
    pub path: String,
}

/// `qrc:/` and the `parts` of a path, each with or without slashes around it.
fn address(parts: &[&str]) -> String {
    let parts = parts
        .iter()
        .map(|part| part.strip_prefix("./").unwrap_or(part).trim_matches('/'));
    format!(
        "qrc:/{}",
        parts
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("/")
    )
}

/// Where a build keeps a module when it does not say.
const PREFIX: &str = "/qt/qml";

#[derive(Debug, PartialEq, Eq)]
pub struct Type {
    pub name: String,
    /// The file, from the directory of what describes the module.
    pub path: String,
}

/// What a `qmldir` describes: None when it names no module, and its
/// directory is only ever imported by its path.
///
/// ```text
/// module Thermostat
/// singleton Constants 1.0 Constants.qml
/// RoomView 1.0 RoomView.qml
/// # block Player 1.0 Player.qml Media "Plays one source"
/// ```
///
/// A `# block` comment declares a functional block (ADR 0003; its grammar
/// and the rest of its words are `ljos_blocks`'). It is a comment so that
/// Qt, which fails an import on a line it does not know, reads past it;
/// here it also declares its type, once even when a plain line says so
/// too, which is what a qmldir shared with Qt writes (Qt sees only that).
pub fn qmldir(text: &str) -> Option<Module> {
    let mut uri = None;
    let mut types: Vec<Type> = Vec::new();
    let mut blocks: Vec<usize> = Vec::new();
    for line in text.lines() {
        if let Some(block) = block_type(line) {
            if !types.contains(&block) {
                blocks.push(types.len());
                types.push(block);
            }
            continue;
        }
        let line = line.split('#').next().unwrap_or(line);
        let mut words: Vec<&str> = line.split_whitespace().collect();
        if let ["module", name] = words.as_slice() {
            uri = Some((*name).to_string());
            continue;
        }
        if matches!(words.first(), Some(&("singleton" | "internal"))) {
            words.remove(0);
        }
        // `Name File.qml`, or with a version between them.
        let (Some(name), Some(path)) = (words.first(), words.last()) else {
            continue;
        };
        let ty = Type {
            name: (*name).to_string(),
            path: (*path).to_string(),
        };
        if matches!(words.len(), 2 | 3)
            && is_type_name(name)
            && path.ends_with(".qml")
            && !blocks.iter().any(|&i| types[i] == ty)
        {
            types.push(ty);
        }
    }
    // A `qmldir` says nothing of what is kept with the module.
    Some(Module {
        uri: uri?,
        types,
        resources: Vec::new(),
        prefix: PREFIX.to_string(),
    })
}

/// The type a `# block Name 1.0 File.qml ...` comment declares.
fn block_type(line: &str) -> Option<Type> {
    let rest = line.trim_start().strip_prefix('#')?.trim_start();
    let rest = rest.strip_prefix("block")?;
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let mut words = rest.split_whitespace();
    let (name, _version, path) = (words.next()?, words.next()?, words.next()?);
    (is_type_name(name) && path.ends_with(".qml")).then(|| Type {
        name: name.to_string(),
        path: path.to_string(),
    })
}

/// The modules a `CMakeLists.txt` makes:
///
/// ```text
/// qt_add_qml_module(app URI Thermostat QML_FILES Main.qml views/RoomView.qml)
/// ```
///
/// A file is the type its name says, as it is to the build. Only what is
/// written there is read: a list in a variable `set` in the same file is
/// followed, anything computed is not.
pub fn cmake(text: &str) -> Vec<Module> {
    build(text).0
}

/// The files a `CMakeLists.txt` keeps in the program by themselves:
///
/// ```text
/// qt_add_resources(app "data" PREFIX "/data" BASE "data" FILES "data/medals.csv")
/// ```
///
/// Each is named by its path from `BASE` under `PREFIX`.
pub fn kept(text: &str) -> Vec<Resource> {
    build(text).1
}

fn build(text: &str) -> (Vec<Module>, Vec<Resource>) {
    let mut variables: HashMap<String, Vec<String>> = HashMap::new();
    for here in ["CMAKE_CURRENT_SOURCE_DIR", "CMAKE_CURRENT_LIST_DIR"] {
        variables.insert(here.to_string(), vec![".".to_string()]);
    }
    let mut modules = Vec::new();
    let mut kept = Vec::new();
    for (name, arguments) in commands(text) {
        let arguments = expand(&arguments, &variables);
        match name.to_ascii_lowercase().as_str() {
            "set" => {
                if let Some((name, values)) = arguments.split_first() {
                    variables.insert(name.clone(), values.to_vec());
                }
            }
            "list" => {
                if let [operation, name, values @ ..] = arguments.as_slice()
                    && operation == "APPEND"
                {
                    variables
                        .entry(name.clone())
                        .or_default()
                        .extend(values.iter().cloned());
                }
            }
            "qt_add_qml_module" | "qt6_add_qml_module" => modules.extend(module(&arguments)),
            "qt_add_resources" | "qt6_add_resources" => kept.extend(resources(&arguments)),
            _ => {}
        }
    }
    (modules, kept)
}

fn resources(arguments: &[String]) -> Vec<Resource> {
    let mut prefix = "";
    let mut base = "";
    let mut files = Vec::new();
    let mut keyword = "";
    for argument in arguments.iter().skip(2) {
        if is_keyword(argument) {
            keyword = argument;
            continue;
        }
        match keyword {
            "PREFIX" => prefix = argument,
            "BASE" => {
                base = argument
                    .strip_prefix("./")
                    .unwrap_or(argument)
                    .trim_end_matches('/')
            }
            "FILES" => files.push(argument),
            _ => {}
        }
    }
    let named = |file: &String| {
        let path = file.strip_prefix("./").unwrap_or(file);
        let from = path
            .strip_prefix(base)
            .and_then(|rest| rest.strip_prefix('/'))
            .unwrap_or(path);
        Resource {
            address: address(&[prefix, from]),
            path: file.clone(),
        }
    };
    files.into_iter().map(named).collect()
}

fn module(arguments: &[String]) -> Option<Module> {
    let mut uri = None;
    let mut types = Vec::new();
    let mut resources = Vec::new();
    let mut prefix = PREFIX.to_string();
    let mut keyword = "";
    for argument in arguments.iter().skip(1) {
        if is_keyword(argument) {
            keyword = argument;
            continue;
        }
        match keyword {
            "URI" => uri = Some(argument.clone()),
            "QML_FILES" => {
                let file = argument.rsplit('/').next().unwrap_or(argument);
                let name = file.split('.').next().unwrap_or(file);
                if file.ends_with(".qml") && is_type_name(name) {
                    types.push(Type {
                        name: name.to_string(),
                        path: argument.clone(),
                    });
                }
            }
            "RESOURCES" => resources.push(argument.clone()),
            "RESOURCE_PREFIX" => prefix = argument.clone(),
            _ => {}
        }
    }
    Some(Module {
        uri: uri?,
        types,
        resources,
        prefix,
    })
}

/// `QML_FILES`, `NO_PLUGIN`: what says what the arguments after it are.
fn is_keyword(argument: &str) -> bool {
    argument.starts_with(|c: char| c.is_ascii_uppercase())
        && argument
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

/// A file is a type when its name starts with a capital: `main.qml` is not.
fn is_type_name(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_uppercase())
}

/// `${NAME}` by itself is the list the variable holds; inside an argument it
/// is its value. One nothing here set is left as it is written.
fn expand(arguments: &[String], variables: &HashMap<String, Vec<String>>) -> Vec<String> {
    let mut expanded = Vec::new();
    for argument in arguments {
        if let Some(name) = argument
            .strip_prefix("${")
            .and_then(|rest| rest.strip_suffix('}'))
            && let Some(values) = variables.get(name)
        {
            expanded.extend(values.iter().cloned());
            continue;
        }
        let mut value = argument.clone();
        for (name, values) in variables {
            let reference = format!("${{{name}}}");
            if value.contains(&reference) {
                value = value.replace(&reference, &values.join(";"));
            }
        }
        expanded.push(value);
    }
    expanded
}

/// The commands of a CMake file, each with its arguments as they are
/// written.
fn commands(text: &str) -> Vec<(String, Vec<String>)> {
    let mut commands = Vec::new();
    let mut chars = text.chars().peekable();
    let mut word = String::new();
    // `if (` is `if(`: a space ends the word, and only another word drops it.
    let mut ended = false;
    while let Some(c) = chars.next() {
        match c {
            '#' => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            '(' => {
                let name = std::mem::take(&mut word);
                let mut arguments = Vec::new();
                let mut argument = String::new();
                let mut depth = 1;
                while let Some(c) = chars.next() {
                    match c {
                        '#' => {
                            for c in chars.by_ref() {
                                if c == '\n' {
                                    break;
                                }
                            }
                        }
                        '"' => {
                            while let Some(c) = chars.next() {
                                match c {
                                    '"' => break,
                                    '\\' => argument.extend(chars.next()),
                                    c => argument.push(c),
                                }
                            }
                            arguments.push(std::mem::take(&mut argument));
                        }
                        '(' => depth += 1,
                        ')' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        c if c.is_whitespace() => {
                            if !argument.is_empty() {
                                arguments.push(std::mem::take(&mut argument));
                            }
                        }
                        c => argument.push(c),
                    }
                }
                if !argument.is_empty() {
                    arguments.push(argument);
                }
                commands.push((name, arguments));
            }
            c if c.is_alphanumeric() || c == '_' => {
                if std::mem::take(&mut ended) {
                    word.clear();
                }
                word.push(c);
            }
            c if c.is_whitespace() => ended = true,
            _ => word.clear(),
        }
    }
    commands
}
