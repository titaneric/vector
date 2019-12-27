import React, {useState} from 'react';
import classnames from 'classnames';
import {MDXProvider} from '@mdx-js/react';
import CodeHeader from '@site/src/components/CodeHeader';
import CodeBlock from '@theme/CodeBlock';

function isObject(a) {
  return (!!a) && (a.constructor === Object);
};

function toTOML(value) {
  return JSON.stringify(value);
}

function keyToTOML(key) {
  if ( key.includes(".") ) {
    return "\"" + key + "\"";
  } else {
    return key;
  }
}

function exampleToTOML(name, example) {
  if (isObject(example)) {
    if ('name' in example && 'value' in example) {
      return `${keyToTOML(example.name)} = ${toTOML(example.value)}`;
    } else {
      return `${keyToTOML(Object.keys(example)[0])} = ${toTOML(Object.values(example)[0])}`
    }
  } else if (name) {
    return `${name} = ${toTOML(example)}`;
  } else {
    return `${toTOML(example)}`;
  }
}

function Enum({values}) {
  let elements = [];

  if (!Array.isArray(values)) {
    for (var key in values) {
      elements.push(<code key={key} title={values[key]}>{toTOML(key)}</code>);
      elements.push(" ");
    }
  } else {
    for (var index in values) {
      let value = values[index];
      elements.push(<code key={value}>{toTOML(value)}</code>);
      elements.push(" ");
    }
  }

  return elements;
}

function Example({name, path, value}) {
  return <code>{exampleToTOML(null, value)}</code>;
}

function Examples({name, path, values}) {
  let code = '';

  values.forEach(function (value) {
    code += (exampleToTOML(name, value) + "\n");
  });

  if (path) {
    code = `[${path}]\n${code}`;
  }

  return (
    <div>
      <CodeHeader fileName="vector.toml" />

      <CodeBlock className="language-toml">
        {code}
      </CodeBlock>
    </div>
  );
}

function RelevantWhen({value}) {
  let relKey = Object.keys(value)[0];
  let relValue = Object.values(value)[0];

  return (
    <span>
      <code><a href={`#${relKey}`}>{relKey}</a></code> = <code>{toTOML(relValue)}</code>
    </span>
  );
}

function FieldFooter({defaultValue, enumValues, examples, name, path, relevantWhen}) {
  const [showExamples, setShowExamples] = useState(false);

  if (defaultValue || enumValues || (examples && examples.length > 0)) {
    return (
      <div className="info">
        {defaultValue !== undefined ?
          (defaultValue ?
            <div>Default: <Example name={name} path={path} value={defaultValue} /></div> :
            <div>No default</div>) :
          null}
        {enumValues ?
          <div>Enum, must be one of: <Enum values={enumValues} /></div> :
          null}
        {relevantWhen ?
          <div>Only relevant when: <RelevantWhen value={relevantWhen} /></div> :
          null}
        <div>
          <div className="show-more" onClick={() => setShowExamples(!showExamples)}>
            {showExamples ? "Hide examples" : "View examples"}
          </div>
          {showExamples && <div className="examples"><Examples name={name} path={path} values={examples} /></div>}
        </div>
      </div>
    );
  } else {
    return null;
  }
}

function Field({children, common, defaultValue, enumValues, examples, name, path, relevantWhen, templateable, type, unit, required}) {
  const [collapse, setCollapse] = useState(false);

  let filteredChildren = children;

  if (collapse) {
    filteredChildren = filteredChildren.filter(child => child.props.originalType != 'p');
  }

  return (
    <div className={classnames('field', 'section', (required ? 'field-required' : ''), (collapse ? 'field-collapsed' : ''))} required={required}>
      <div className="badges">
        {common && <span className="badge badge--primary" title="This is a popular that we recommend for getting started">common</span>}
        {templateable && <span className="badge badge--primary" title="This option is dynamic and accepts the Vector template syntax">templateable</span>}
        <span className="badge badge--secondary">{type}</span>
        {enumValues && Object.keys(enumValues).length > 0 && <span className="badge badge--secondary" title="This option is an enumation and only allows specific values">enum</span>}
        {unit && <span className="badge badge--secondary">{unit}</span>}
        {required ?
          <span className="badge badge--danger">required</span> :
          <span className="badge badge--secondary">optional</span>}
      </div>
      {filteredChildren}
      {!collapse &&
        <FieldFooter defaultValue={defaultValue} enumValues={enumValues} examples={examples} name={name} path={path} relevantWhen={relevantWhen} />}
    </div>
  );
}

export default Field;
