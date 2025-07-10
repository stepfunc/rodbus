const path = require('path');
const {visit} = require('unist-util-visit');

module.exports = function mermaid(options = {}) {
    return (tree, file) => {
        let importAdded = false;
        visit(tree, 'code', (node, index, parent) => {
            if(node.lang === 'mermaid') {
                node.type = 'jsx';
                // Properly escape the chart content for JSX
                const escapedChart = node.value
                    .replace(/\\/g, '\\\\')
                    .replace(/"/g, '\\"')
                    .replace(/\n/g, '\\n')
                    .replace(/\r/g, '\\r');
                node.value = `<Mermaid chart={"${escapedChart}"} />`;

                if(!importAdded) {
                    const importPath = path.relative(file.dirname, path.resolve(__dirname, '../../src/theme/Mermaid.js')).replace(/\\/g, '/');
                    const importNode = {
                        type: 'import',
                        value: `import Mermaid from '${importPath}'`,
                    }
                    parent.children.splice(index, 0, importNode);
                    importAdded = true;

                    return index + 1
                }
            }
        });
  };
}
