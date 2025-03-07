use napi_derive::napi;
use rspack_core::{
  Builtins, CodeGeneration, Define, Minification, MinificationCondition, MinificationConditions,
  PluginExt, PresetEnv, Provide,
};
use rspack_error::internal_error;
use rspack_plugin_banner::{BannerConfig, BannerPlugin};
use rspack_plugin_copy::CopyPlugin;
use rspack_plugin_css::{plugin::CssConfig, CssPlugin};
use rspack_plugin_dev_friendly_split_chunks::DevFriendlySplitChunksPlugin;
use rspack_plugin_html::HtmlPlugin;
use rspack_plugin_progress::ProgressPlugin;
use serde::Deserialize;

use crate::JsLoaderRunner;

mod raw_banner;
mod raw_copy;
mod raw_css;
mod raw_decorator;
mod raw_html;
mod raw_plugin_import;
mod raw_postcss;
mod raw_progress;
mod raw_react;
mod raw_relay;

pub use raw_css::*;
pub use raw_decorator::*;
pub use raw_html::*;
pub use raw_postcss::*;
pub use raw_progress::*;
pub use raw_react::*;

use self::{
  raw_banner::RawBannerConfig, raw_copy::RawCopyConfig, raw_plugin_import::RawPluginImportConfig,
  raw_relay::RawRelayConfig,
};
use crate::RawOptionsApply;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[napi(object)]
pub struct RawMinificationCondition {
  #[napi(ts_type = r#""string" | "regexp""#)]
  pub r#type: String,
  pub string_matcher: Option<String>,
  pub regexp_matcher: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[napi(object)]
pub struct RawMinificationConditions {
  #[napi(ts_type = r#""string" | "regexp" | "array""#)]
  pub r#type: String,
  pub string_matcher: Option<String>,
  pub regexp_matcher: Option<String>,
  pub array_matcher: Option<Vec<RawMinificationCondition>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[napi(object)]
pub struct RawMinification {
  pub passes: u32,
  pub drop_console: bool,
  pub pure_funcs: Vec<String>,
  pub extract_comments: Option<String>,
  pub test: Option<RawMinificationConditions>,
  pub include: Option<RawMinificationConditions>,
  pub exclude: Option<RawMinificationConditions>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[napi(object)]
pub struct RawPresetEnv {
  pub targets: Vec<String>,
  #[napi(ts_type = "'usage' | 'entry'")]
  pub mode: Option<String>,
  pub core_js: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[napi(object)]
pub struct RawCodeGeneration {
  pub keep_comments: bool,
}

impl TryFrom<RawMinification> for Minification {
  type Error = rspack_error::Error;

  fn try_from(value: RawMinification) -> rspack_error::Result<Self> {
    fn try_condition(
      raw_condition: Option<RawMinificationConditions>,
    ) -> Result<Option<MinificationConditions>, rspack_error::Error> {
      let condition: Option<MinificationConditions> = if let Some(test) = raw_condition {
        Some(test.try_into()?)
      } else {
        None
      };

      Ok(condition)
    }

    Ok(Self {
      passes: value.passes as usize,
      drop_console: value.drop_console,
      pure_funcs: value.pure_funcs,
      extract_comments: value.extract_comments,
      test: try_condition(value.test)?,
      include: try_condition(value.include)?,
      exclude: try_condition(value.exclude)?,
    })
  }
}

impl TryFrom<RawMinificationCondition> for MinificationCondition {
  type Error = rspack_error::Error;

  fn try_from(x: RawMinificationCondition) -> rspack_error::Result<Self> {
    let result = match x.r#type.as_str() {
      "string" => Self::String(x.string_matcher.ok_or_else(|| {
        internal_error!(
          "should have a string_matcher when MinificationConditions.type is \"string\""
        )
      })?),
      "regexp" => Self::Regexp(rspack_regex::RspackRegex::new(
        &x.regexp_matcher.ok_or_else(|| {
          internal_error!(
            "should have a regexp_matcher when MinificationConditions.type is \"regexp\""
          )
        })?,
      )?),
      _ => panic!(
        "Failed to resolve the condition type {}. Expected type is `string`, `regexp` or `array`.",
        x.r#type
      ),
    };

    Ok(result)
  }
}

impl TryFrom<RawMinificationConditions> for MinificationConditions {
  type Error = rspack_error::Error;

  fn try_from(value: RawMinificationConditions) -> rspack_error::Result<Self> {
    let result: MinificationConditions = match value.r#type.as_str() {
      "string" => Self::String(value.string_matcher.ok_or_else(|| {
        internal_error!("should have a string_matcher when MinificationConditions.type is \"string\"")
      })?),
      "regexp" => Self::Regexp(rspack_regex::RspackRegex::new(
        &value.regexp_matcher.ok_or_else(|| {
          internal_error!(
            "should have a regexp_matcher when MinificationConditions.type is \"regexp\""
          )
        })?,
      )?),
      "array" => Self::Array(
        value.array_matcher
          .ok_or_else(|| {
            internal_error!(
              "should have a array_matcher when MinificationConditions.type is \"array\""
            )
          })?
          .into_iter()
          .map(|i| i.try_into())
          .collect::<rspack_error::Result<Vec<_>>>()?,
      ),
      _ => panic!(
        "Failed to resolve the MinificationContions type {}. Expected type is `string`, `regexp`, `array`.",
        value.r#type
      ),
    };

    Ok(result)
  }
}

impl From<RawPresetEnv> for PresetEnv {
  fn from(raw_preset_env: RawPresetEnv) -> Self {
    Self {
      targets: raw_preset_env.targets,
      mode: raw_preset_env.mode.and_then(|mode| match mode.as_str() {
        "usage" => Some(swc_core::ecma::preset_env::Mode::Usage),
        "entry" => Some(swc_core::ecma::preset_env::Mode::Entry),
        _ => None,
      }),
      core_js: raw_preset_env.core_js,
    }
  }
}

impl From<RawCodeGeneration> for CodeGeneration {
  fn from(raw_code_generation: RawCodeGeneration) -> Self {
    Self {
      keep_comments: raw_code_generation.keep_comments,
    }
  }
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[napi(object)]
pub struct RawBuiltins {
  pub html: Option<Vec<RawHtmlPluginConfig>>,
  pub css: Option<RawCssPluginConfig>,
  pub postcss: Option<RawPostCssConfig>,
  pub minify_options: Option<RawMinification>,
  pub preset_env: Option<RawPresetEnv>,
  #[napi(ts_type = "Record<string, string>")]
  pub define: Define,
  #[napi(ts_type = "Record<string, string[]>")]
  pub provide: Provide,
  pub tree_shaking: String,
  pub progress: Option<RawProgressPluginConfig>,
  pub react: RawReactOptions,
  pub decorator: Option<RawDecoratorOptions>,
  pub no_emit_assets: bool,
  pub emotion: Option<String>,
  pub dev_friendly_split_chunks: bool,
  pub copy: Option<RawCopyConfig>,
  pub banner: Option<Vec<RawBannerConfig>>,
  pub plugin_import: Option<Vec<RawPluginImportConfig>>,
  pub relay: Option<RawRelayConfig>,
  pub code_generation: Option<RawCodeGeneration>,
}

impl RawOptionsApply for RawBuiltins {
  type Options = Builtins;

  fn apply(
    self,
    plugins: &mut Vec<rspack_core::BoxPlugin>,
    _: &JsLoaderRunner,
  ) -> Result<Self::Options, rspack_error::Error> {
    if let Some(htmls) = self.html {
      for html in htmls {
        plugins.push(HtmlPlugin::new(html.into()).boxed());
      }
    }
    if let Some(css) = self.css {
      let options = CssConfig {
        targets: self
          .preset_env
          .as_ref()
          .map(|preset_env| preset_env.targets.clone())
          .unwrap_or_default(),
        postcss: self.postcss.unwrap_or_default().into(),
        modules: css.modules.try_into()?,
      };
      plugins.push(CssPlugin::new(options).boxed());
    }
    if let Some(progress) = self.progress {
      plugins.push(ProgressPlugin::new(progress.into()).boxed());
    }
    if self.dev_friendly_split_chunks {
      plugins.push(DevFriendlySplitChunksPlugin::new().boxed());
    }
    if let Some(copy) = self.copy {
      plugins.push(CopyPlugin::new(copy.patterns.into_iter().map(Into::into).collect()).boxed());
    }

    if let Some(banners) = self.banner {
      let configs: Vec<BannerConfig> = banners
        .into_iter()
        .map(|banner| banner.try_into())
        .collect::<rspack_error::Result<Vec<_>>>()?;

      configs
        .into_iter()
        .for_each(|banner| plugins.push(BannerPlugin::new(banner).boxed()));
    }

    Ok(Builtins {
      minify_options: self.minify_options.map(|i| i.try_into()).transpose()?,
      preset_env: self.preset_env.map(Into::into),
      define: self.define,
      provide: self.provide,
      tree_shaking: self.tree_shaking.into(),
      react: self.react.into(),
      decorator: self.decorator.map(|i| i.into()),
      no_emit_assets: self.no_emit_assets,
      dev_friendly_split_chunks: self.dev_friendly_split_chunks,
      emotion: self
        .emotion
        .map(|i| serde_json::from_str(&i))
        .transpose()
        .map_err(|e| internal_error!(e.to_string()))?,
      plugin_import: self
        .plugin_import
        .map(|plugin_imports| plugin_imports.into_iter().map(Into::into).collect()),
      relay: self.relay.map(Into::into),
      code_generation: self.code_generation.map(Into::into),
    })
  }
}
