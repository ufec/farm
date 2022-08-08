#![feature(box_patterns)]

use std::{collections::HashMap, sync::Arc};

use farmfe_core::{
  config::Config,
  context::CompilationContext,
  error::CompilationError,
  module::ModuleId,
  parking_lot::Mutex,
  plugin::{
    Plugin, PluginHookContext, PluginLoadHookParam, PluginLoadHookResult, PluginResolveHookParam,
    PluginResolveHookResult,
  },
  resource::resource_pot::{JsResourcePotMetaData, ResourcePotMetaData, ResourcePotType},
  swc_common::DUMMY_SP,
  swc_ecma_ast::{
    CallExpr, Expr, ExprOrSpread, ExprStmt, Lit, Module as SwcModule, ModuleDecl, ModuleItem, Stmt,
    Str,
  },
};
use farmfe_toolkit::{
  fs::read_file_utf8,
  script::{merge_module_asts_of_resource_pot, module_type_from_id, parse_module, parse_stmt},
  swc_ecma_parser::{EsConfig, Syntax},
};

const RUNTIME_SUFFIX: &str = ".farm-runtime";

/// FarmPluginRuntime is charge of:
/// * resolving, parsing and generating a executable runtime code and inject the code into the entries.
/// * merge module's ast and render the script module using farm runtime's specification, for example, wrap the module to something like `function(module, exports, require) { xxx }`, see [Farm Runtime RFC](https://github.com/farm-fe/rfcs/pull/1)
///
/// The runtime supports html entry and script(js/jsx/ts/tsx) entry, when entry is html, the runtime will be injected as a inline <script /> tag in the <head /> tag;
/// when entry is script, the runtime will be injected into the entry module's head, makes sure the runtime execute before all other code.
///
/// All runtime module (including the runtime core and its plugins) will be suffixed as `.farm-runtime` to distinguish with normal script modules.
/// ```
pub struct FarmPluginRuntime {
  runtime_ast: Mutex<Option<SwcModule>>,
}

impl Plugin for FarmPluginRuntime {
  fn name(&self) -> &str {
    "FarmPluginRuntime"
  }

  fn config(&self, config: &mut Config) -> farmfe_core::error::Result<Option<()>> {
    config.input.insert(
      "runtime".to_string(),
      format!("{}{}", config.runtime.path, RUNTIME_SUFFIX),
    );
    println!("{}{}", config.runtime.path, RUNTIME_SUFFIX);
    Ok(Some(()))
  }

  fn resolve(
    &self,
    param: &PluginResolveHookParam,
    context: &Arc<CompilationContext>,
    hook_context: &PluginHookContext,
  ) -> farmfe_core::error::Result<Option<PluginResolveHookResult>> {
    // avoid cyclic resolve
    if matches!(&hook_context.caller, Some(c) if c == "FarmPluginRuntime") {
      Ok(None)
    } else if param.source.ends_with(RUNTIME_SUFFIX) {
      let ori_source = param.source.replace(RUNTIME_SUFFIX, "");
      let resolve_result = context.plugin_driver.resolve(
        &PluginResolveHookParam {
          source: ori_source,
          ..param.clone()
        },
        &context,
        &PluginHookContext {
          caller: Some(String::from("FarmPluginRuntime")),
          meta: HashMap::new(),
        },
      )?;

      if let Some(mut res) = resolve_result {
        res.id = format!("{}{}", res.id, RUNTIME_SUFFIX);
        Ok(Some(res))
      } else {
        Ok(None)
      }
    } else {
      Ok(None)
    }
  }

  fn load(
    &self,
    param: &PluginLoadHookParam,
    _context: &Arc<CompilationContext>,
    _hook_context: &PluginHookContext,
  ) -> farmfe_core::error::Result<Option<PluginLoadHookResult>> {
    if param.id.ends_with(RUNTIME_SUFFIX) {
      let real_file_path = param.id.replace(RUNTIME_SUFFIX, "");
      let content = read_file_utf8(&real_file_path)?;

      Ok(Some(PluginLoadHookResult {
        content,
        module_type: module_type_from_id(&real_file_path).ok_or_else(|| {
          CompilationError::GenericError(
            "Unsupported file type of runtime, only support `js/jsx/ts/tsx/mjs/cjs`".to_string(),
          )
        })?,
      }))
    } else {
      Ok(None)
    }
  }

  fn process_resource_pot_graph(
    &self,
    resource_pot_graph: &mut farmfe_core::resource::resource_pot_graph::ResourcePotGraph,
    context: &Arc<CompilationContext>,
  ) -> farmfe_core::error::Result<Option<()>> {
    let mut module_graph = context.module_graph.write();

    for resource_pot in resource_pot_graph.resource_pots_mut() {
      if resource_pot.id.to_string().ends_with(RUNTIME_SUFFIX) {
        let rendered_resource_pot_ast =
          merge_module_asts_of_resource_pot(resource_pot, &mut *module_graph, context);

        #[cfg(not(windows))]
        let minimal_runtime = include_str!("./js-runtime/minimal-runtime.js");
        #[cfg(windows)]
        let minimal_runtime = include_str!(".\\js-runtime\\minimal-runtime.js");

        let mut runtime_ast = parse_module(
          "farm-internal-minimal-runtime",
          minimal_runtime,
          Syntax::Es(EsConfig::default()),
          context.meta.script.cm.clone(),
        )?;

        if let ModuleItem::Stmt(Stmt::Expr(ExprStmt {
          expr: box Expr::Call(CallExpr { args, .. }),
          ..
        })) = &mut runtime_ast.body[0]
        {
          args[0] = ExprOrSpread {
            spread: None,
            expr: Box::new(Expr::Object(rendered_resource_pot_ast)),
          };
          args[1] = ExprOrSpread {
            spread: None,
            expr: Box::new(Expr::Lit(Lit::Str(Str {
              span: DUMMY_SP,
              value: ModuleId::from(resource_pot.id.to_string().as_str())
                .id(context.config.mode.clone())
                .into(),
              raw: None,
            }))),
          };
        }

        resource_pot.resource_pot_type = ResourcePotType::Runtime;

        self.runtime_ast.lock().replace(runtime_ast);
        break;
      }
    }

    Ok(Some(()))
  }

  fn render_resource_pot(
    &self,
    resource_pot: &mut farmfe_core::resource::resource_pot::ResourcePot,
    context: &Arc<CompilationContext>,
  ) -> farmfe_core::error::Result<Option<()>> {
    // the runtime module and its plugins should be in the same resource pot
    if matches!(resource_pot.resource_pot_type, ResourcePotType::Js) {
      let mut module_graph = context.module_graph.write();
      let rendered_resource_pot_ast =
        merge_module_asts_of_resource_pot(resource_pot, &mut *module_graph, context);

      #[cfg(not(windows))]
      let wrapper = include_str!("./js-runtime/resource-wrapper.js");
      #[cfg(windows)]
      let wrapper = include_str!(".\\js-runtime\\resource-wrapper.js");

      let mut wrapper_ast = parse_module(
        "farm-internal-resource-wrapper",
        wrapper,
        Syntax::Es(EsConfig::default()),
        context.meta.script.cm.clone(),
      )?;

      if let ModuleItem::Stmt(Stmt::Expr(ExprStmt {
        expr: box Expr::Call(CallExpr { args, .. }),
        ..
      })) = &mut wrapper_ast.body[0]
      {
        args[0] = ExprOrSpread {
          spread: None,
          expr: Box::new(Expr::Object(rendered_resource_pot_ast)),
        };
      }

      resource_pot.meta = ResourcePotMetaData::Js(JsResourcePotMetaData { ast: wrapper_ast });

      Ok(Some(()))
    } else {
      Ok(None)
    }
  }

  fn generate_resources(
    &self,
    resource_pot: &mut farmfe_core::resource::resource_pot::ResourcePot,
    context: &Arc<CompilationContext>,
    hook_context: &PluginHookContext,
  ) -> farmfe_core::error::Result<Option<Vec<farmfe_core::resource::Resource>>> {
    if matches!(&hook_context.caller, Some(c) if c == self.name()) {
      return Ok(None);
    }

    // only handle runtime resource pot and entry resource pot
    if matches!(resource_pot.resource_pot_type, ResourcePotType::Runtime) {
      // do not emit anything of Runtime, as it will be generated and injected when generating entry resources
      Ok(Some(vec![]))
    } else if let Some(entry_module_id) = &resource_pot.entry_module {
      // modify the ast according to the type,
      // if js, insert the runtime ast in the front
      match resource_pot.resource_pot_type {
        ResourcePotType::Js => {
          let mut runtime_ast = self.runtime_ast.lock().take().unwrap();

          let resource_pot_ast = &mut resource_pot.meta.as_js_mut().ast;
          resource_pot_ast.body.insert(0, runtime_ast.body.remove(0));

          // TODO support top level await, and only support reexport default export now, should support more in the future
          // call the entry module
          let call_entry = parse_module(
            "farm-internal-call-entry-module",
            &format!(
              r#"const entry = globalThis.__acquire_farm_module_system__().require("{}").default;export default entry;"#,
              entry_module_id.id(context.config.mode.clone())
            ),
            Syntax::Es(Default::default()),
            context.meta.script.cm.clone(),
          )?;

          resource_pot_ast.body.extend(call_entry.body);
        }
        _ => { /* only inject entry execution for script, html entry will be injected after all resources generated */
        }
      }

      Ok(None)
    } else {
      Ok(None)
    }
  }
}

impl FarmPluginRuntime {
  pub fn new(_: &Config) -> Self {
    Self {
      runtime_ast: Mutex::new(None),
    }
  }
}
