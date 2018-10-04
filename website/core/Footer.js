/**
 * Copyright (c) 2017-present, Facebook, Inc.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

const React = require('react');

class Footer extends React.Component {
  docUrl(doc, language) {
    const baseUrl = this.props.config.baseUrl;
    return `${baseUrl}docs/${language ? `${language}/` : ''}${doc}`;
  }

  pageUrl(doc, language) {
    const baseUrl = this.props.config.baseUrl;
    return baseUrl + (language ? `${language}/` : '') + doc;
  }

  render() {
    return (
      <footer className="nav-footer" id="footer">
        <section className="sitemap">
          <div>
            <h5>Docs</h5>
            <a href={this.docUrl('guides.html', this.props.language)}>
              Guides
            </a>
            <a href={this.docUrl('docs.html', this.props.language)}>
              API / CLI documentation
            </a>
          </div>
          <div>
            <h5>Community</h5>
            <a
              href="https://twitter.com/RlayOfficial"
              target="_blank"
              rel="noreferrer noopener">
              Twitter
            </a>
            <a
              href="https://t.me/rlay_official"
              target="_blank"
              rel="noreferrer noopener">
              Telegram
            </a>
          </div>
          <div>
            <h5>More</h5>
            <a href={`https://rlay.com`}>Rlay website</a>
            <a href={`https://medium.com/rlay-official`}>Blog</a>
            <a href="https://github.com/rlay-project">GitHub organization</a>
            <a href="https://github.com/rlay-project/rlay-client">GitHub project</a>
            <a
              className="github-button"
              href={this.props.config.repoUrl}
              data-icon="octicon-star"
              data-count-href="/rlay-project/rlay-client/stargazers"
              data-show-count="true"
              data-count-aria-label="# stargazers on GitHub"
              aria-label="Star this project on GitHub">
              Star
            </a>
          </div>
        </section>

        <section className="copyright">{this.props.config.copyright}</section>
      </footer>
    );
  }
}

module.exports = Footer;
