const fs = require('fs');
const path = require('path');

const intermediate = require('./intermediate.json');
const outputPath = path.resolve(__dirname, '../../docs/generated/rlay-ontology-entities.md');

const addHeader = (output) => {
  output.file += '---\n';
  output.file += 'id: rlay-ontology-entities\n';
  output.file += 'title: Entity Kinds\n';
  output.file += 'sidebar_label: Entity Kinds\n';
  output.file += '---\n';
}

const addIntro = (output) => {
  output.file += '\n';
  output.file += '<AUTOGENERATED_TABLE_OF_CONTENTS>\n';
  output.file += '\n';
}

const addEntities = (output) => {
  output.file += '\n';
  intermediate.kinds.forEach((kind) => {
    output.file += `### \`${kind.name}\`\n`;
    output.file += '\n';
    output.file += `**CID prefix:** \`0x01${kind.cidPrefixHex}\`\n\n`;
    output.file += `#### Fields\n`;
    kind.fields.forEach((field) => {
      const requiredPart = field.required ? ' **(required)**' : '';
      let kind = field.kind;
      if (kind === 'IRI') {
        kind = 'CID';
      }
      output.file += `- ${field.name}: ${kind}${requiredPart}\n`;
    })
  })
}

const main = () => {
  const output = { file: '' };

  addHeader(output);
  addIntro(output);
  addEntities(output);

  fs.writeFile(outputPath, output.file, function(err) {
    if (err) throw err;
  });
};

main();
