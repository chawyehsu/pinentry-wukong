# Changelog

## 0.1.0 (2026-09-02)


### Features

* **config:** add TOML configuration system ([#2](https://github.com/chawyehsu/pinentry-wukong/issues/2)) ([2238465](https://github.com/chawyehsu/pinentry-wukong/commit/22384650dbddece2af2c2c1c5d8fa13751a7eac1))
* display caller feedback ([#5](https://github.com/chawyehsu/pinentry-wukong/issues/5)) ([bc5ef34](https://github.com/chawyehsu/pinentry-wukong/commit/bc5ef34ca079318c84c1853c2bef4adcba9bab2d))
* first implementation ([be0ac20](https://github.com/chawyehsu/pinentry-wukong/commit/be0ac203df4138430f9fe3ccd522ea1e9f5b09ce))
* **tui:** add up/down arrow key navigation ([85d51f5](https://github.com/chawyehsu/pinentry-wukong/commit/85d51f5a5cc065eade2414966a034a797498bf26))
* windows support ([#1](https://github.com/chawyehsu/pinentry-wukong/issues/1)) ([7b30d9a](https://github.com/chawyehsu/pinentry-wukong/commit/7b30d9a15d021223d250abb88b07ab2ac8373ffb))


### Bug Fixes

* correct windows console reader/writer access mask ([7376d28](https://github.com/chawyehsu/pinentry-wukong/commit/7376d2833e618fa729de17df776830b6a26272d3))
* handle windows keyboard ([547f35a](https://github.com/chawyehsu/pinentry-wukong/commit/547f35a8a7f49875de2b2baf4bd9367de53cbcb8))
* **pinentry:** clear rejected cached passphrase on error ([cef3371](https://github.com/chawyehsu/pinentry-wukong/commit/cef3371177f31b378d18df8093c48d4dca90e255))
* **server:** return error instead of panicking on invalid command ([0936dbb](https://github.com/chawyehsu/pinentry-wukong/commit/0936dbb90d63d15d4e97c827d2bc8b37f4a87053))
* **tui:** drop terminal before virtual terminal guard ([4d6b39e](https://github.com/chawyehsu/pinentry-wukong/commit/4d6b39e88b6dba344c5403ea80737b450c4df5a1))
* **tui:** use alternate screen ([#4](https://github.com/chawyehsu/pinentry-wukong/issues/4)) ([7a7075a](https://github.com/chawyehsu/pinentry-wukong/commit/7a7075aca04ea82a2e5e89d17e2cfa063a781cef))
* **tui:** use native Windows console I/O ([#3](https://github.com/chawyehsu/pinentry-wukong/issues/3)) ([6755c41](https://github.com/chawyehsu/pinentry-wukong/commit/6755c41e9b856cc19def1a626073aa47fbedaaee))


### Continuous Integration

* add windows arm target ([259ed70](https://github.com/chawyehsu/pinentry-wukong/commit/259ed70a2f0e50e994f54ba8aa3351fd6a19542d))
