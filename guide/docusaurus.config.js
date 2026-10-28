// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

const path = require('path');
const samplePlugin = require('./plugins/sample');
const sitedata = require('./sitedata.json');

const {themes} = require('prism-react-renderer');
const vsLight = themes.vsLight; 

module.exports = {
  title: `Rodbus ${sitedata.version}`,
  tagline: 'Pretty sure we don\'t need this page, just the docs',
  url: 'https://docs.stepfunc.io',
  baseUrl: `/rodbus/${sitedata.version}/guide/`,
  onBrokenLinks: 'throw',
  onBrokenMarkdownLinks: 'warn',
  favicon: 'images/brand/favicon.png',
  organizationName: 'stepfunc', // Usually your GitHub org/user name.
  projectName: 'rodbus', // Usually your repo name.
  themeConfig: {
    prism: {
      theme: vsLight,
      additionalLanguages: ['rust', 'java', 'csharp', 'cmake'],
    },
    colorMode: {
      defaultMode: 'light',
      disableSwitch: true,
    },
    mermaid: {
      theme: {
        light: 'base',
        dark: 'dark',
      },
      options: {
        themeVariables: {
          primaryColor: '#e4f3ff',
          primaryTextColor: '#004e98',
          primaryBorderColor: '#004e98',
          lineColor: '#0056a7',
          background: '#ffffff',
          mainBkg: '#e4f3ff',
          secondBkg: '#f5f6f7',
          nodeBkg: '#e4f3ff',
          nodeTextColor: '#004e98',
          edgeLabelBackground: '#ffffff',
          actorBkg: '#e4f3ff',
          actorBorder: '#004e98',
          actorTextColor: '#004e98',
          signalColor: '#004e98',
          signalTextColor: '#004e98',

          // Groups (subgraphs)
          clusterBkg: '#f9f9fa',
          clusterBorder: '#9db7d5',
        },
        sequence: {mirrorActors: false},
      },
    },
    navbar: {
      title: `Rodbus ${sitedata.version}`,
      logo: {
        alt: 'Logo',
        src: 'images/brand/logo.svg',
        href: '/docs/guide'
      },
      items: [
        {
          type: 'dropdown',
          label: 'API docs',
          position: 'right',
          items: [
            {label: 'Rust', href: `https://docs.stepfunc.io/rodbus/${sitedata.version}/rust/rodbus/`},
            {label: 'C', href: `https://docs.stepfunc.io/rodbus/${sitedata.version}/c/rodbus_8h.html`},
            {label: 'C++', href: `https://docs.stepfunc.io/rodbus/${sitedata.version}/cpp/rodbus_8hpp.html`},
            {label: 'Java', href: `https://docs.stepfunc.io/rodbus/${sitedata.version}/java/`},
            {label: '.NET', href: `https://docs.stepfunc.io/rodbus/${sitedata.version}/dotnet/namespacerodbus.html`},
          ],
        },
        {label: 'stepfunc.io', href: 'https://stepfunc.io/products/libraries/modbus/', position: 'right'},
        {label: 'GitHub', href: sitedata.github_url, position: 'right'},
      ],
    },
    footer: {
      style: 'dark',
      logo: {
        alt: 'Step Function',
        src: 'images/brand/logo.svg',
      },
      links: [
        {
          title: 'Step Function I/O',
          items: [
            {
              label: 'Products',
              href: 'https://stepfunc.io/products/',
            },
            {
              label: 'Blog',
              to: 'https://stepfunc.io/blog/',
            },
          ],
        },
        {
          title: 'Library',
          items: [
            {
              label: 'GitHub',
              href: sitedata.github_url,
            },
            {
              label: 'Homepage',
              href: 'https://stepfunc.io/products/libraries/modbus/',
            },
          ],
        },
        {
          title: 'Modbus',
          items: [
            {
              label: 'Modbus.org',
              to: 'https://modbus.org/',
            }
          ],
        },
      ],
      copyright: `Copyright © ${new Date().getFullYear()} Step Function I/O LLC`,
    },
  },
  stylesheets: [
    'https://fonts.googleapis.com/css2?family=Barlow:wght@500;600&display=swap',
  ],
  presets: [
    [
      '@docusaurus/preset-classic',
      {
        docs: {
          sidebarPath: require.resolve('./sidebars.js'),
          remarkPlugins: [
            samplePlugin,
          ],
        },
        theme: {
          customCss: require.resolve('./src/css/custom.css'),
        },
      },
    ],
  ],
  plugins: [
    path.resolve(__dirname, './plugins/changelog'),
    [
      '@docusaurus/plugin-client-redirects',
      {
        redirects: [
          {
            from: '/docs/api/master/tls_client',
            to: '/docs/api/client/tls_client',
          },
          {
            from: '/docs/api/outstation/tls_server',
            to: '/docs/api/server/tls_server',
          },
          {
            from: '/docs/api/outstation/database',
            to: '/docs/api/server/database',
          },
          {
            from: '/docs/api/outstation/write_handler',
            to: '/docs/api/server/write_handler',
          },
        ],
      },
    ],
  ],
  themes: ['@docusaurus/theme-mermaid'],
  markdown: {
    mermaid: true,
  },
};
