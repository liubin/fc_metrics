use serde::Serialize;
use std::collections::HashMap;
use std::env;
use std::fmt::{self, Display};
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process;
use tinytemplate::TinyTemplate;

static TEMPLATE: &'static str = r#"// Copyright (c) 2020 xxx.yyy 
//
// SPDX-License-Identifier: Apache-2.0
//
// WARNING: This file is auto-generated - DO NOT EDIT!

package virtcontainers

import (
    "github.com/prometheus/client_golang/prometheus"
)

const fcMetricsNS = "kata_firecracker"

// prometheus metrics Firecracker exposed.
var (
{{ for line in metrics_var_declare_stmt }}{line}
{{ endfor }}
)

// registerFirecrackerMetrics register all metrics to prometheus.
func registerFirecrackerMetrics() \{
{{ for line in metrics_register_stmt }}{line}
{{ endfor }}
}

// updateFirecrackerMetrics update all metrics to the latest values.
func updateFirecrackerMetrics(fm *FirecrackerMetrics) \{
{{ for line in metrics_set_stmt }}{line}
{{ endfor }}
}

{{ for line in metrics_struct_declare_stmt }}{line}
{{ endfor }}
"#;

enum GenerateError {
    IncorrectUsage,
    ReadFile(io::Error),
    ParseError(syn::Error),
    RenderError(tinytemplate::error::Error),
}

impl Display for GenerateError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::GenerateError::*;
        match self {
            IncorrectUsage => write!(f, "Usage: fc-metrics-generator path/to/filename.rs"),
            ReadFile(error) => write!(f, "Failed to read file: {}", error),
            ParseError(error) => write!(f, "Failed to parse source file: {}", error),
            RenderError(error) => write!(f, "Failed to render source file: {}", error),
        }
    }
}

struct RustStruct {
    comments: Vec<String>,
    name: String,
    // struct_item: syn::ItemStruct,
    fields: Vec<StructField>,
}

struct StructField {
    /// var name for golang
    var_name: String,
    /// var type for golang
    var_type: String,
    comments: Vec<String>,
}

fn strip_comment(s: &mut String) -> &mut String {
    // remove the last double quota
    s.pop();
    // remove the first `= "`
    s.replace_range(0..=2, "");
    s
}

fn json_tag(s: &String) -> String {
    format!("`json:\"{}\"`", s)
}

fn rust_field_name_to_go(s: &String) -> String {
    let vv: Vec<String> = s.split('_').map(|x| to_uppercase(&x)).collect::<Vec<_>>();
    vv.join("")
}

fn to_lowercase(s: &str) -> String {
    s.chars()
        .take(1)
        .flat_map(char::to_lowercase)
        .chain(s.chars().skip(1))
        .collect::<String>()
}

fn to_uppercase(s: &str) -> String {
    s.chars()
        .take(1)
        .flat_map(char::to_uppercase)
        .chain(s.chars().skip(1))
        .collect::<String>()
}

fn go_var_type(s: &String) -> String {
    if s == "SharedMetric" {
        return "uint64".to_string();
    }
    return s.to_string();
}

impl RustStruct {
    fn metric_var_name(&self) -> String {
        to_lowercase(&self.name)
    }

    fn generate_struct_definition_code(&self, vec: &mut Vec<String>, comments: &Vec<String>) {
        for c in comments {
            vec.push(format!("// {}", c));
        }
        vec.push(format!("type {} struct {{", self.name));
        for f in &self.fields {
            for c in &f.comments {
                vec.push(format!("    //{}", c));
            }
            vec.push(format!(
                "    {} {} {}",
                rust_field_name_to_go(&f.var_name),
                go_var_type(&f.var_type),
                json_tag(&f.var_name)
            ));
        }
        vec.push(format!("}}"));
        vec.push("".to_string());
    }

    fn generate_declare_metric_code(&self, vec: &mut Vec<String>, name: &String, help: &String) {
        vec.push(format!(
            r#"{} = prometheus.NewGaugeVec(prometheus.GaugeOpts{{
            Namespace: fcMetricsNS,
            Name:      "{}",
            Help:      "{}",
        }},
            []string{{"item"}},
        )"#,
            self.metric_var_name(),
            name,
            help
        ));
        vec.push("".to_string());
    }

    fn generate_register_code(&self, vec: &mut Vec<String>) {
        vec.push(format!(
            "    prometheus.MustRegister({})",
            self.metric_var_name()
        ))
    }

    fn generate_set_values_code(&self, vec: &mut Vec<String>, field_name: &String) {
        vec.push(format!("    // set metrics for {}", self.name));
        for f in &self.fields {
            vec.push(format!(
                "    {}.WithLabelValues(\"{}\").Set(float64(fm.{}.{}))",
                self.metric_var_name(),
                f.var_name,
                field_name,
                rust_field_name_to_go(&f.var_name)
            ));
        }
        vec.push("".to_string());
    }
}

fn main() {
    if let Err(error) = try_main() {
        let _ = writeln!(io::stderr(), "{}", error);
        process::exit(1);
    }
}

fn try_main() -> Result<(), GenerateError> {
    let mut args = env::args_os();
    let _ = args.next();

    let filepath = match (args.next(), args.next()) {
        (Some(arg), None) => PathBuf::from(arg),
        _ => return Err(GenerateError::IncorrectUsage),
    };

    let code = fs::read_to_string(&filepath).map_err(GenerateError::ReadFile)?;
    let syntax = syn::parse_file(&code).map_err(GenerateError::ParseError)?;

    // parse source file
    let struct_list = parse_source_code(&syntax);

    // parse metrics constructs and generate metrics definitions,
    // register statements, set statements.
    let context = parse_source_tree(struct_list);

    // render to go source file.
    render(context).map_err(GenerateError::RenderError)?;

    Ok(())
}

#[derive(Serialize)]
struct Context {
    metrics_var_declare_stmt: Vec<String>,
    metrics_register_stmt: Vec<String>,
    metrics_set_stmt: Vec<String>,
    metrics_struct_declare_stmt: Vec<String>,
}

fn render(context: Context) -> Result<(), tinytemplate::error::Error> {
    let mut tt = TinyTemplate::new();
    tt.add_template("metrics", TEMPLATE)?;

    tt.set_default_formatter(&tinytemplate::format_unescaped);

    let rendered = tt.render("metrics", &context)?;
    println!("{}", rendered);

    Ok(())
}

fn parse_source_tree(struct_list: HashMap<String, RustStruct>) -> Context {
    let mut metrics_var_declare_stmt: Vec<String> = Vec::new();
    let mut metrics_register_stmt: Vec<String> = Vec::new();
    let mut metrics_set_stmt: Vec<String> = Vec::new();
    let mut metrics_struct_declare_stmt: Vec<String> = Vec::new();

    match struct_list.get(&"FirecrackerMetrics".to_string()) {
        Some(root_struct) => {
            // generate struct for FirecrackerMetrics
            root_struct.generate_struct_definition_code(
                &mut &mut metrics_struct_declare_stmt,
                &root_struct.comments,
            );

            for f in &root_struct.fields {
                match struct_list.get(&f.var_type) {
                    Some(metric_struct) => {
                        metric_struct.generate_struct_definition_code(
                            &mut &mut metrics_struct_declare_stmt,
                            &f.comments,
                        );
                        let help = metric_struct.comments.join(" ").trim().to_string();
                        metric_struct.generate_declare_metric_code(
                            &mut metrics_var_declare_stmt,
                            &f.var_name,
                            &help,
                        );
                        metric_struct.generate_register_code(&mut metrics_register_stmt);
                        metric_struct.generate_set_values_code(
                            &mut metrics_set_stmt,
                            &rust_field_name_to_go(&f.var_name),
                        );
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }

    // prepare for render template
    Context {
        metrics_var_declare_stmt: metrics_var_declare_stmt,
        metrics_register_stmt: metrics_register_stmt,
        metrics_set_stmt: metrics_set_stmt,
        metrics_struct_declare_stmt: metrics_struct_declare_stmt,
    }
}

fn parse_source_code(syntax: &syn::File) -> HashMap<String, RustStruct> {
    let mut struct_list = HashMap::new();

    for item in syntax.items.iter() {
        match item {
            syn::Item::Struct(struct_item) => {
                let mut struct_comments = vec![];
                for attr in &struct_item.attrs {
                    if &attr.path.segments.first().unwrap().ident.to_string() == "doc" {
                        let mut c = attr.tokens.to_string();
                        c = strip_comment(&mut c).to_string();
                        struct_comments.push(c);
                    }
                }
                // only process struct item
                match &struct_item.fields {
                    syn::Fields::Named(named_field) => {
                        // and structs with named fields
                        if named_field.named.len() == 0 {
                            continue;
                        }
                        let struct_name = struct_item.ident.to_string();
                        let mut st = RustStruct {
                            name: struct_name.clone(),
                            // struct_item: struct_item.to_owned(),
                            fields: vec![],
                            comments: struct_comments,
                        };

                        for nt in named_field.named.iter() {
                            match &nt.vis {
                                syn::Visibility::Public(_) => {}
                                _ => {
                                    // skip not-pub fields
                                    continue;
                                }
                            }

                            let var_type;
                            let var_name = nt.ident.as_ref().unwrap().to_string();
                            match &nt.ty {
                                syn::Type::Path(tp) => {
                                    let sg = tp.path.segments.first().unwrap();
                                    var_type = sg.ident.to_string();
                                }
                                _ => {
                                    // skip other types
                                    continue;
                                }
                            }

                            // process doc.( start with `///` )
                            let mut comments = vec![];
                            for attr in &nt.attrs {
                                if &attr.path.segments.first().unwrap().ident.to_string() == "doc" {
                                    let mut c = attr.tokens.to_string();
                                    c = strip_comment(&mut c).to_string();
                                    comments.push(c);
                                }
                            }

                            let field = StructField {
                                var_name: var_name,
                                var_type: var_type,
                                comments: comments,
                            };
                            st.fields.push(field);
                        }
                        struct_list.insert(struct_name, st);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    struct_list
}
