#![feature(let_chains)]

use rspack_core::{
  Compilation, EntryDependency, EntryOptions, MakeParam, ModuleDependency, Plugin, PluginContext,
  PluginMakeHookOutput,
};

#[derive(Debug)]
pub struct EntryPlugin {
  name: String,
  options: EntryOptions,
  entry_request: String,
}

impl EntryPlugin {
  pub fn new(name: String, entry_request: String, options: EntryOptions) -> Self {
    Self {
      name,
      options,
      entry_request,
    }
  }
}
// 这个make方法在给定的编译环境中添加一个新的入口依赖项，并在需要时强制构建这个依赖项。
#[async_trait::async_trait]
impl Plugin for EntryPlugin {
  async fn make(
    &self,
    _ctx: PluginContext,
    compilation: &mut Compilation,
    param: &mut MakeParam,
  ) -> PluginMakeHookOutput {
    // 1. 首先，它检查是否存在一个增量重建状态，并且这不是第一次构建。如果满足这些条件，它会立即返回Ok(())。
    if let Some(state) = compilation.options.get_incremental_rebuild_make_state() && !state.is_first() {
      return Ok(());
    }
     // 2. 然后，它创建一个新的EntryDependency，并将其添加到compilation的模块图中，返回一个dependency_id。
    let dependency = Box::new(EntryDependency::new(self.entry_request.clone())); // "./src/index.js"
    let dependency_id = dependency.id(); // 0
    compilation.add_entry(*dependency_id, self.name.clone(), self.options.clone());  // 3. 接着，它将新的依赖项添加到compilation的入口中。
    param.add_force_build_dependency(*dependency_id, None); // id:0, 'main', option
    compilation.module_graph.add_dependency(dependency); // 4. 最后，它将新的依赖项添加到param的强制构建依赖项中，并返回Ok(())。

    param.add_force_build_dependency(dependency_id, None);
    Ok(())
  }
}
