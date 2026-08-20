module.exports = function tailwindPlugin(_context: any, _options: any) {
    return {
        name: "tailwind-plugin",
        configurePostCss(postcssOptions: { plugins: any[] }) {
            postcssOptions.plugins = [require("@tailwindcss/postcss")];
            return postcssOptions;
        },
    };
};
