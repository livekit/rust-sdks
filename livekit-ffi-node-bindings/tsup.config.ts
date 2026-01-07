import type { Options } from "tsup";

const defaultOptions: Options = {
  entry: ["src/**/*.ts", "!src/**/*.test.ts"],
  format: ["cjs", "esm"],
  splitting: false,
  sourcemap: true,
  dts: true,
  clean: true,
  bundle: false,
  target: "node16",
  external: [/\.\/.*\.cjs/, /\.\/.*.node/],
  esbuildOptions: (options, context) => {
    if (context.format === "esm") {
      options.packages = "external";
    }
  },
  plugins: [
    {
      // https://github.com/egoist/tsup/issues/953#issuecomment-2294998890
      // ensuring that all local requires/imports in `.cjs` files import from `.cjs` files.
      // require('./path') → require('./path.cjs') in `.cjs` files
      // require('../path') → require('../path.cjs') in `.cjs` files
      // from './path' → from './path.cjs' in `.cjs` files
      // from '../path' → from '../path.cjs' in `.cjs` files
      name: "fix-cjs-imports",
      renderChunk(code) {
        if (this.format === "cjs") {
          const regexCjs =
            /require\((?<quote>['"])(?<import>\.[^'"]+)\.js['"]\)/g;
          const regexDynamic =
            /import\((?<quote>['"])(?<import>\.[^'"]+)\.js['"]\)/g;
          const regexEsm =
            /from(?<space>[\s]*)(?<quote>['"])(?<import>\.[^'"]+)\.js['"]/g;
          return {
            code: code
              .replace(regexCjs, "require($<quote>$<import>.cjs$<quote>)")
              .replace(regexDynamic, "import($<quote>$<import>.cjs$<quote>)")
              .replace(regexEsm, "from$<space>$<quote>$<import>.cjs$<quote>"),
          };
        }
      },
    },
  ],
};
export default defaultOptions;
