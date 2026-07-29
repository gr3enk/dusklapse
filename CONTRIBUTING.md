## Contributing to Dusklapse

Hi, thank you for your interest in helping to improve Dusklapse. Before you get started, here are a few rules and guidelines.

### Commits

To ensure consistency in commits, the following structure should be followed:

The commit message should be structured as follows: `<prefix>: <description>`. For example: `fix: nikon cameras lost connection after performing xy`

The following prefixes are preferred but not mandatory. If none of the prefixes listed below apply to a commit, you may deviate from them. However, 99% of commits fall under the prefixes listed:

| Prefix | Description                                                                        |
| ------ | ---------------------------------------------------------------------------------- |
| feat   | Implementing a new feature                                                         |
| fix    | Fixing a bug or issue                                                              |
| chore  | Minor changes that have no impact on the production logic, such as code formatting |
| test   | Changes to unit tests, e2e tests etc.                                              |
| docs   | Documentation changes                                                              |

### Pull Requests

A pull request should have a single, clear objective. A large feature can also be split across several pull requests. The title of the pull request should summarise briefly but concisely what has been changed, as this will be included in the changelog. The changes should be detailed in the pull request description. Screenshots may also be used for this purpose. Merging pull requests requires that all tests have run successfully.

### Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/)
- [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode)
- [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)
- [Prettier](https://marketplace.cursorapi.com/items/?itemName=esbenp.prettier-vscode)
- [Rust](https://marketplace.cursorapi.com/items/?itemName=rust-lang.rust)
