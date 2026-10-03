export default {
  hooks: {
    beforePacking(pkg) {
      delete pkg.devDependencies;
      delete pkg.scripts;

      // Add publication metadata
      pkg.publishedAt = new Date().toISOString();

      return pkg;
    },
  },
};
