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
        },
      },
    },
    navbar: {
      title: `Rodbus ${sitedata.version}`,
      logo: {
        alt: 'Logo',
        src: 'images/brand/logo.svg',
        href: '/docs/guide'
      },
      items: [],
    },
    footer: {
      logo: {
        alt: 'Step Function',
        src: 'images/brand/footer-logo.svg',
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
  plugins: [path.resolve(__dirname, './plugins/changelog')],
  themes: ['@docusaurus/theme-mermaid'],
  markdown: {
    mermaid: true,
  },
};
