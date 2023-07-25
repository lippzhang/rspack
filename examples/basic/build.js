const {rspack: webpack} = require('@rspack/core')
const config = require('./rspack.config')

// 执行 webpack 函数有传回调函数
// const compiler = webpack(config, (err, stats) => {
//   if (err) {
//     console.log(err)
//   }
// })

// 执行 webpack 函数没有有传回调函数
const compiler = webpack(config)
// 需要手动调用一下 compiler.run
compiler.run((err, stats) => {
  stats.toString();
  if (err) {
    console.log(err)
  }
})
