<?xml version="1.0" encoding="UTF-8"?>
<xsl:stylesheet version="1.0" xmlns:xsl="http://www.w3.org/1999/XSL/Transform">
  <xsl:output method="html" indent="yes" encoding="UTF-8"  version="4.0" />
  <xsl:param name="version" />
  <xsl:param name="assetRoot" />
  <xsl:param name="apiRoot" />
  <xsl:param name="wsURL" />
  <xsl:param name="loginURL" />
  <xsl:param name="logoutURL" />
  <xsl:param name="uiRoot" />
  <xsl:template match="page">
    <html>
      <head>
        <title>costanza</title>
        <link rel="stylesheet" href="https://cdnjs.cloudflare.com/ajax/libs/font-awesome/6.2.0/css/all.min.css" />
        <link rel="stylesheet" href="{$assetRoot}main.css?v={$version}" />

        <link rel="apple-touch-icon" sizes="57x57" href="{$assetRoot}apple-icon-57x57.png" />
        <link rel="apple-touch-icon" sizes="60x60" href="{$assetRoot}apple-icon-60x60.png" />
        <link rel="apple-touch-icon" sizes="72x72" href="{$assetRoot}apple-icon-72x72.png" />
        <link rel="apple-touch-icon" sizes="76x76" href="{$assetRoot}apple-icon-76x76.png" />
        <link rel="apple-touch-icon" sizes="114x114" href="{$assetRoot}apple-icon-114x114.png" />
        <link rel="apple-touch-icon" sizes="120x120" href="{$assetRoot}apple-icon-120x120.png" />
        <link rel="apple-touch-icon" sizes="144x144" href="{$assetRoot}apple-icon-144x144.png" />
        <link rel="apple-touch-icon" sizes="152x152" href="{$assetRoot}apple-icon-152x152.png" />
        <link rel="apple-touch-icon" sizes="180x180" href="{$assetRoot}apple-icon-180x180.png" />
        <link rel="icon" type="image/png" sizes="32x32" href="{$assetRoot}favicon-32x32.png" />
        <link rel="icon" type="image/png" sizes="16x16" href="{$assetRoot}favicon-16x16.png" />

        <meta property="og:image" content="{$assetRoot}art-350x350.png" />
        <meta property="og:title" content="costanza printer control center" />

        <meta name="version" content="{$version}" />
        <meta name="apiRoot" content="{$apiRoot}" />
        <meta name="wsURL" content="{$wsURL}" />
        <meta name="uiRoot" content="{$uiRoot}" />
        <meta name="loginURL" content="{$loginURL}" />
        <meta name="logoutURL" content="{$logoutURL}" />
        <meta name="viewport" content="width=device-width, initial-scale=1, maximum-scale=1, minimum-scale=1" />
      </head>
      <body>
        <script type="text/javascript" src="{$assetRoot}main.js?v={$version}"></script>
        <script type="text/javascript" src="{$assetRoot}boot.js?v={$version}"></script>
      </body>
    </html>
  </xsl:template>
</xsl:stylesheet>
