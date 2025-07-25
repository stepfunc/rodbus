# Rodbus Documentation Website

This website is built using [Docusaurus 3](https://docusaurus.io/), a modern static website generator.

## Ubuntu Setup

Install Node.js and npm:

```bash
# Update package index
sudo apt update

# Install Node.js v20 (recommended) or v18.19+ (minimum)
# For Node.js v20 (recommended):
curl -fsSL https://deb.nodesource.com/setup_20.x | sudo -E bash -
sudo apt install -y nodejs

# For Node.js v18 (if v20 is not available):
# curl -fsSL https://deb.nodesource.com/setup_18.x | sudo -E bash -
# sudo apt install -y nodejs

# Verify installation
node --version
npm --version

```

## Installation

Install the project dependencies:

```console
npm install
```

## Local Development

Start the development server:

```console
npm run start
```

This command starts a local development server and opens a browser window. Most changes are reflected live without having to restart the server.

## Build

Generate static content:

```console
npm run build
```

This command generates static content into the `build` directory and can be served using any static contents hosting service.