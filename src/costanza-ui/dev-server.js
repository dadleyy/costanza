const http = require('http');
const url = require('url');
const path = require('path');
const fs = require('fs');
const httpProxy = require('http-proxy');
const dotenv = require('dotenv');

// Quick-n-dirty static file development server to satisfy local development needs; development
// on this file itself is not really worth the time as long as it "just works".
//
// General list of cleanup todos:
//
// 1. actually implement a better command line parsing method
// 2. figure out what the `serveUnder` stuff is _actually_doing.

dotenv.config();

const beetleServerAddr = process.env['MILTON_SRV_ADDR'] || 'http://0.0.0.0:8081';
const parsedServerAddr = url.parse(beetleServerAddr);

const port = process.env['MILTON_UI_PROXY_PORT'] || 8338;

const proxy = httpProxy.createProxyServer({});
const buildTargetName = process.argv.includes('--release') ? 'release' : 'debug';
const serveUnder = process.argv.includes('--serve-under');
console.log(`serveUnder? ${serveUnder}`);

const server = http.createServer(function(request, response) {
  if (request.url.startsWith('/api')) {
    console.info(`proxying request '${request.url}' to '${beetleServerAddr}'`);
    const target = {
      host: parsedServerAddr.hostname,
      port: parsedServerAddr.port,
      path: request.url.slice('/api'.length),
    };
    proxy.web(request, response, { target, ignorePath: true }, function (error) {
      console.error(`non-terminal proxy error - ${error.message}`);
    });
    return;
  }

  const [urlPath] = request.url.split('?');
  const resource = serveUnder
    ? urlPath.replace('/printing', '') 
    : urlPath;

  const staticPath = path.join(__dirname, 'target', buildTargetName, resource);

  fs.stat(staticPath, function (error, stats) {
    const resolvedPath = !error && stats.isFile()
      ? staticPath
      : path.join(__dirname, 'target', buildTargetName, 'index.html');

    console.info(`attempting to serve static file from '${resolvedPath}' (from '${staticPath}')`);

    fs.readFile(resolvedPath, function (error, data) {
      if (error) {
        console.warn(`unable to read ${resolvedPath} - ${error}`);
        response.writeHead(500);
        response.end();

        return;
      }

      response.writeHead(200);
      response.end(data);
    });
  });
});

console.info(`development server listening on port '${port}'`)
server.listen(port);

