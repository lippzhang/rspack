(self['webpackChunkwebpack'] = self['webpackChunkwebpack'] || []).push([["main"], {
"./app.js": function (__unused_webpack_module, __webpack_exports__, __webpack_require__) {
'use strict';
__webpack_require__.r(__webpack_exports__);
__webpack_require__.d(__webpack_exports__, {
  'b': function() { return b; }
});

var __WEBPACK_DEFAULT_EXPORT__ = /* "./lib" unused */null;
 const b = 1;
},
"./index.js": function (__unused_webpack_module, __webpack_exports__, __webpack_require__) {
'use strict';
__webpack_require__.r(__webpack_exports__);
/* harmony import */var _app__WEBPACK_IMPORTED_MODULE_0_ = __webpack_require__(/* ./app */"./app.js");

_app__WEBPACK_IMPORTED_MODULE_0_["b"];
},

},function(__webpack_require__) {
var __webpack_exec__ = function(moduleId) { return __webpack_require__(__webpack_require__.s = moduleId) }
var __webpack_exports__ = (__webpack_exec__("./index.js"));

}
]);