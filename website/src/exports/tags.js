import GithubSlugger from 'github-slugger';

function blogColor(category) {
  switch(category) {
    case 'domain':
      return 'blue';

    case 'type':
      return 'pink'

    default:
      return 'primary';
  }
}

function guidesColor(category) {
  switch(category) {
    case 'domain':
      return 'blue';

    case 'type':
      return 'pink'

    default:
      return 'primary';
  }
}

function enrichTag(tag, colorProfile) {
  const labelParts = tag.label.split(': ', 2);
  const category = labelParts[0];
  const value = labelParts[1];

  let style = 'primary';

  switch(colorProfile) {
    case 'blog':
      style = blogColor(category);
      break;

     case 'guides':
       style = guidesColor(category);
       break;
  }

  return {
    category: category,
    count: tag.count,
    label: tag.label,
    permalink: tag.permalink,
    style: style,
    value: value,
  };
}

export function enrichTags(tags, colorProfile) {
  const slugger = new GithubSlugger();

  return tags.map(tag => {
    let normalizedTag = tag;

    if (typeof(tag) == 'string') {
      normalizedTag = {label: tag, permalink: `/blog/tags/${slugger.slug(tag)}`};
    }

    return enrichTag(normalizedTag, colorProfile)
  });
}

export function extractTagValue(tags, category) {
  let prefix = category + ': ';

  let tag = tags.find(tag => tag.startsWith(prefix));

  if (tag) {
    return tag.replace(prefix, '');
  } else {
    return null;
  }
}

export default {enrichTags, extractTagValue};
