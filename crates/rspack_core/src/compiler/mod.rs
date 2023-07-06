mod compilation;
mod hmr;
mod make;
mod queue;
mod resolver;

use std::{path::Path, sync::Arc};

pub use compilation::*;
pub use make::MakeParam;
pub use queue::*;
pub use resolver::*;
use rspack_error::Result;
use rspack_fs::AsyncWritableFileSystem;
use rspack_futures::FuturesResults;
use rspack_identifier::IdentifierSet;
use rustc_hash::FxHashMap as HashMap;
use tracing::instrument;

use crate::{
  cache::Cache, fast_set, AssetEmittedArgs, CompilerOptions, Plugin, PluginDriver,
  SharedPluginDriver,
};

#[derive(Debug)]
pub struct Compiler<T>
where
  T: AsyncWritableFileSystem + Send + Sync,
{
  pub options: Arc<CompilerOptions>,
  pub output_filesystem: T,
  pub compilation: Compilation,
  pub plugin_driver: SharedPluginDriver,
  pub resolver_factory: Arc<ResolverFactory>,
  pub cache: Arc<Cache>,
  /// emitted asset versions
  /// the key of HashMap is filename, the value of HashMap is version
  pub emitted_asset_versions: HashMap<String, String>,
}

impl<T> Compiler<T>
where
  T: AsyncWritableFileSystem + Send + Sync,
{
  #[instrument(skip_all)]
  pub fn new(
    options: CompilerOptions,
    plugins: Vec<Box<dyn Plugin>>,
    output_filesystem: T,
  ) -> Self {
    let options = Arc::new(options);
    // 解析器工厂
    let resolver_factory = Arc::new(ResolverFactory::new(options.resolve.clone()));
    // 插件驱动
    let plugin_driver = Arc::new(PluginDriver::new(
      options.clone(),
      plugins,
      resolver_factory.clone(),
    ));
    let cache = Arc::new(Cache::new(options.clone()));

    Self {
      options: options.clone(),
      compilation: Compilation::new(
        options,
        Default::default(),
        plugin_driver.clone(),
        resolver_factory.clone(),
        cache.clone(),
      ),
      output_filesystem,
      plugin_driver,
      resolver_factory,
      cache,
      emitted_asset_versions: Default::default(),
    }
  }

  pub async fn run(&mut self) -> Result<()> {
    self.build().await?;
    Ok(())
  }
  // 进入build过程
  #[instrument(name = "build", skip_all)]
  pub async fn build(&mut self) -> Result<()> {
    // 结束缓存的空闲状态。
    self.cache.end_idle();
    // TODO: clear the outdate cache entries in resolver,
    // TODO: maybe it's better to use external entries.
    // 清除解析器工厂中的过期缓存条目。
    self.plugin_driver.resolver_factory.clear_entries();

    // 创建一个新的编译实例，并将其设置为当前编译实例
    fast_set(
      &mut self.compilation,
      Compilation::new(
        self.options.clone(),
        Default::default(),
        self.plugin_driver.clone(),
        self.resolver_factory.clone(),
        self.cache.clone(),
      ),
    );
    // 调用插件驱动的 before_compile 钩子函数，这个函数在编译开始之前执行。
    self.plugin_driver.before_compile().await?;

    // Fake this compilation as *currently* rebuilding does not create a new compilation
    // 调用插件驱动的 this_compilation 钩子函数
    self
      .plugin_driver
      .this_compilation(&mut self.compilation)
      .await?;
    // 调用插件驱动的 compilation 钩子函数，这个函数在编译过程中执行。
    self
      .plugin_driver
      .compilation(&mut self.compilation)
      .await?;
    // 执行编译过程，参数 MakeParam::ForceBuildDeps(Default::default()) 表示强制构建依赖项。
    self
      .compile(MakeParam::ForceBuildDeps(Default::default()))
      .await?;
    self.cache.begin_idle(); // 开始缓存的空闲状态。
    self.compile_done().await?; // 调用 compile_done 函数，表示编译过程完成。输出内容，emit阶段
    Ok(())
  }

  // 这段代码定义了一个异步函数 compile，它接受一个 MakeParam 类型的参数，并返回一个 Result 类型的结果
  #[instrument(name = "compile", skip_all)]
  async fn compile(&mut self, params: MakeParam) -> Result<()> {
    let option = self.options.clone();
    self.compilation.make(params).await?; // 开始编译 make阶段
                                          // 调用插件驱动的 finish_make 钩子函数
    self
      .plugin_driver
      .finish_make(&mut self.compilation)
      .await?;
    // 调用插件驱动的 finish 钩子函数
    self.compilation.finish(self.plugin_driver.clone()).await?;
    // by default include all module in final chunk 默认情况下，将所有模块包含在最终的 chunk 中。
    self.compilation.include_module_ids = self
      .compilation
      .module_graph
      .modules()
      .keys()
      .cloned()
      .collect::<IdentifierSet>();
    // tree shaking阶段
    if option.builtins.tree_shaking.enable()
      || option
        .output
        .enabled_library_types
        .as_ref()
        .map(|types| {
          types
            .iter()
            .any(|item| item == "module" || item == "commonjs-static")
        })
        .unwrap_or(false)
    {
      // 优化依赖并分解结果。
      let (analyze_result, diagnostics) = self
        .compilation
        .optimize_dependency()
        .await?
        .split_into_parts();
      if !diagnostics.is_empty() {
        // 如果诊断结果不为空，将其推入 self.compilation 的批量诊断中。
        self.compilation.push_batch_diagnostic(diagnostics);
      }
      //  更新 self.compilation 的各种属性，如 used_symbol_ref，bailout_module_identifiers，side_effects_free_modules，module_item_map 等。
      self.compilation.used_symbol_ref = analyze_result.used_symbol_ref;
      self.compilation.bailout_module_identifiers = analyze_result.bail_out_module_identifiers;
      self.compilation.side_effects_free_modules = analyze_result.side_effects_free_modules;
      self.compilation.module_item_map = analyze_result.module_item_map;
      //  如果启用了 self.options.builtins.tree_shaking 和 self.options.optimization.side_effects，则更新 self.compilation.include_module_ids。
      if self.options.builtins.tree_shaking.enable()
        && self.options.optimization.side_effects.is_enable()
      {
        self.compilation.include_module_ids = analyze_result.include_module_ids;
      }
      // 更新 self.compilation.optimize_analyze_result_map
      self.compilation.optimize_analyze_result_map = analyze_result.analyze_results;
    }
    // 开始 seal阶段, 生产环境相关优化的阶段
    self.compilation.seal(self.plugin_driver.clone()).await?;
    // 调用 钩子函数
    self
      .plugin_driver
      .after_compile(&mut self.compilation)
      .await?;

    // Consume plugin driver diagnostic 获取插件驱动的诊断结果，并将其推入 self.compilation 的批量诊断中。
    let plugin_driver_diagnostics = self.plugin_driver.take_diagnostic();
    self
      .compilation
      .push_batch_diagnostic(plugin_driver_diagnostics);

    Ok(())
  }
  // emit_assets阶段
  #[instrument(name = "compile_done", skip_all)]
  async fn compile_done(&mut self) -> Result<()> {
    if !self.compilation.options.builtins.no_emit_assets {
      self.emit_assets().await?;
    }

    self.compilation.done(self.plugin_driver.clone()).await?;
    Ok(())
  }
  //  这里貌似看起来也是使用的 nodejs中的fs模块。
  #[instrument(name = "emit_assets", skip_all)]
  pub async fn emit_assets(&mut self) -> Result<()> {
    if self.options.output.clean {
      if self.emitted_asset_versions.is_empty() {
        self
          .output_filesystem
          .remove_dir_all(&self.options.output.path)
          .await?;
      } else {
        // clean unused file
        let assets = self.compilation.assets();
        let _ = self
          .emitted_asset_versions
          .iter()
          .filter_map(|(filename, _version)| {
            if !assets.contains_key(filename) {
              let file_path = Path::new(&self.options.output.path).join(filename);
              Some(self.output_filesystem.remove_file(file_path))
            } else {
              None
            }
          })
          .collect::<FuturesResults<_>>();
      }
    }
    // 插件钩子函数
    self.plugin_driver.emit(&mut self.compilation).await?;

    let mut new_emitted_asset_versions = HashMap::default();
    let results = self
      .compilation
      .assets()
      .iter()
      .filter_map(|(filename, asset)| {
        // collect version info to new_emitted_asset_versions
        if self.options.is_incremental_rebuild_emit_asset_enabled() {
          new_emitted_asset_versions.insert(filename.to_string(), asset.info.version.clone());
        }

        if let Some(old_version) = self.emitted_asset_versions.get(filename) {
          if old_version.as_str() == asset.info.version && !old_version.is_empty() {
            return None;
          }
        }

        Some(self.emit_asset(&self.options.output.path, filename, asset))
      })
      .collect::<FuturesResults<_>>();

    self.emitted_asset_versions = new_emitted_asset_versions;
    // return first error
    for item in results.into_inner() {
      item?;
    }

    self.plugin_driver.after_emit(&mut self.compilation).await
  }

  async fn emit_asset(
    &self,
    output_path: &Path,
    filename: &str,
    asset: &CompilationAsset,
  ) -> Result<()> {
    if let Some(source) = asset.get_source() {
      let filename = filename
        .split_once('?')
        .map(|(filename, _query)| filename)
        .unwrap_or(filename);
      let file_path = Path::new(&output_path).join(filename);
      self
        .output_filesystem
        .create_dir_all(
          file_path
            .parent()
            .unwrap_or_else(|| panic!("The parent of {} can't found", file_path.display())),
        )
        .await?;
      self
        .output_filesystem
        .write(&file_path, source.buffer())
        .await?;

      self.compilation.emitted_assets.insert(filename.to_string());

      let asset_emitted_args = AssetEmittedArgs {
        filename,
        output_path,
        source: source.clone(),
        target_path: file_path.as_path(),
        compilation: &self.compilation,
      };
      self
        .plugin_driver
        .asset_emitted(&asset_emitted_args)
        .await?;
    }
    Ok(())
  }
}
