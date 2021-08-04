const fs = require('fs');
const cueJsonOutput = "data/docs.json";
const chalk = require('chalk');
const TOML = require('@iarna/toml');
const YAML = require('yaml');

// Helper functions
const getArrayValue = (obj) => {
  const enumVal = (obj.enum != null) ? [Object.keys(obj.enum)[0]] : null;

  const examplesVal = (obj.examples != null && obj.examples.length > 0) ? [obj.examples[0]] : null;

  return obj.default || examplesVal || enumVal || null;
}

const getValue = (obj) => {
  const enumVal = (obj.enum != null) ? Object.keys(obj.enum)[0] : null;

  const examplesVal = (obj.examples != null && obj.examples.length > 0) ? obj.examples[0] : null;

  return obj.default || examplesVal || enumVal || null;
}

// Convert object to TOML string
const toToml = (obj) => {
  return TOML.stringify(obj);
}

// Convert object to YAML string
const toYaml = (obj) => {
  return `---\n${YAML.stringify(obj)}`;
}

// Convert object to JSON string (indented)
const toJson = (obj) => {
  return JSON.stringify(obj, null, 2);
}

const getExampleValue = (param, deepFilter) => {
  let value;

  Object.keys(param.type).forEach(k => {
    const p = param.type[k];

    if (['array', 'object'].includes(k)) {
      const topType = k;

      if (p.items && p.items.type) {
        const typeInfo = p.items.type;

        Object.keys(typeInfo).forEach(k => {
          if (['array', 'object'].includes(k)) {
            const subType = k;
            const options = typeInfo[k].options;

            var subObj = {};

            Object
              .keys(options)
              .filter(k => deepFilter(options[k]))
              .forEach(k => {
                Object.keys(options[k].type).forEach(key => {
                  const deepTypeInfo = options[k].type[key];

                  if (subType === 'array') {
                    subObj[k] = getArrayValue(deepTypeInfo);
                  } else {
                    subObj[k] = getValue(deepTypeInfo);
                  }
                });
              });

            value = subObj;
          } else {
            if (topType === 'array') {
              value = getArrayValue(typeInfo[k]);
            } else {
              value = getValue(typeInfo[k]);
            }
          }
        });
      }
    } else {
      value = getValue(p);
    }
  });

  return value;
}

Object.makeExampleParams = (params, filter, deepFilter) => {
  var obj = {};

  Object
    .keys(params)
    .filter(k => filter(params[k]))
    .forEach(k => {
      const paramName = k;
      const p = params[k];

      obj[paramName] = {};

      Object.keys(p['type']).forEach(k => {
        if (k === 'object') {
          const options = p['type']['object']['options'];
          Object.keys(options).forEach(name => {
            const optionName = name;
            const fullKey = `${paramName}.${optionName}`;
            const option = options[name];
            Object.keys(option['type']).forEach(k => {
              const typeInfo = option['type'][k];
              const exampleVal = getValue(typeInfo);

              if (exampleVal) {
                obj[paramName][optionName] = {};
                obj[paramName][optionName] = exampleVal
              }
            });
          });
        } else {
          obj[paramName] = getExampleValue(p, deepFilter);
        }
      });
    });

  return obj;
}

// Convert the use case examples (`component.examples`) into multi-format
const makeUseCaseExamples = (component) => {
  if (component.examples) {
    var useCases = [];
    const kind = component.kind;
    const kindPlural = `${kind}s`;
    const keyName = `my_${kind}_id`;

    component.examples.forEach((example) => {
      const config = example.configuration;
      const extra = Object.fromEntries(Object.entries(config).filter(([_, v]) => v != null));

      let exampleConfig;

      if (["transform", "sink"].includes(component.kind)) {
        exampleConfig = {
          [kindPlural]: {
            [keyName]: {
              "type": component.type,
              inputs: ['my-source-or-transform-id'],
              ...extra
            }
          }
        }
      } else {
        exampleConfig = {
          [kindPlural]: {
            [keyName]: {
              "type": component.type,
              ...extra
            }
          }
        }
      }

      useCase = {
        title: example.title,
        description: example.description,
        configuration: {
          toml: toToml(exampleConfig),
          yaml: toYaml(exampleConfig),
          json: toJson(exampleConfig),
        },
        input: example.input,
        output: example.output,
      }

      useCases.push(useCase);
    });

    return useCases;
  } else {
    return null;
  }
}

const main = () => {
  try {
    const debug = process.env.DEBUG === "true" || false;
    const data = fs.readFileSync(cueJsonOutput, 'utf8');
    const docs = JSON.parse(data);
    const components = docs.components;

    console.log(chalk.blue("Creating example configurations for all Vector components..."));

    // Sources, transforms, sinks
    for (const kind in components) {
      console.log(chalk.blue(`Creating examples for ${kind}...`));

      const componentsOfKind = components[kind];

      // Specific components
      for (const componentType in componentsOfKind) {
        const component = componentsOfKind[componentType];
        const configuration = component.configuration;

        const commonParams = Object.makeExampleParams(
          configuration,
          p => p.required || p.common,
          p => p.required || p.common,
        );
        const advancedParams = Object.makeExampleParams(
          configuration,
          _ => true,
          p => p.required || p.common || p.relevant_when,
        );
        const useCaseExamples = makeUseCaseExamples(component);

        const keyName = `my_${kind.substring(0, kind.length - 1)}_id`;

        let commonExampleConfig, advancedExampleConfig;

        // Sinks and transforms are treated differently because they need an `inputs` field
        if (['sinks', 'transforms'].includes(kind)) {
          commonExampleConfig = {
            [kind]: {
              [keyName]: {
                "type": componentType,
                inputs: ['my-source-or-transform-id'],
                ...commonParams,
              }
            }
          };

          advancedExampleConfig = {
            [kind]: {
              [keyName]: {
                "type": componentType,
                inputs: ['my-source-or-transform-id'],
                ...advancedParams,
              }
            }
          };
        } else {
          commonExampleConfig = {
            [kind]: {
              [keyName]: {
                "type": componentType,
                ...commonParams,
              }
            }
          };

          advancedExampleConfig = {
            [kind]: {
              [keyName]: {
                "type": componentType,
                ...advancedParams,
              }
            }
          };
        }

        // A debugging statement to make sure things are going basically as planned
        if (debug) {
          const debugComponent = "aws_ec2_metadata";
          const debugKind = "transforms";

          if (componentType === debugComponent && kind === debugKind) {
            console.log(
              chalk.blue(`Printing debug JSON for the ${debugComponent} ${debugKind.substring(0, debugKind.length - 1)}...`));

            console.log(JSON.stringify(advancedExampleConfig, null, 2));
          }
        }

        docs['components'][kind][componentType]['examples'] = useCaseExamples;

        docs['components'][kind][componentType]['example_configs'] = {
          common: {
            toml: toToml(commonExampleConfig),
            yaml: toYaml(commonExampleConfig),
            json: toJson(commonExampleConfig),
          },
          advanced: {
            toml: toToml(advancedExampleConfig),
            yaml: toYaml(advancedExampleConfig),
            json: toJson(advancedExampleConfig),
          },
        };
      }
    }

    console.log(chalk.green("Success. Finished generating examples for all components."));
    console.log(chalk.blue(`Writing generated examples as JSON to ${cueJsonOutput}...`));

    fs.writeFileSync(cueJsonOutput, JSON.stringify(docs), 'utf8');

    console.log(chalk.green(`Success. Finished writing example configs to ${cueJsonOutput}.`));
  } catch (err) {
    console.error(err);
  }
}

main();
