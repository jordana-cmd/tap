# Zero-dependency static server for the harness (PowerShell HttpListener).
# Usage:  powershell -ExecutionPolicy Bypass -File harness\serve.ps1
# Routes: /*        → harness/
#         /plans/*  → fixtures/plans/   (confidential fixtures; localhost only)
param([int]$Port = 8787)
$root = $PSScriptRoot
$plans = [IO.Path]::GetFullPath((Join-Path $root "..\fixtures\plans"))
$listener = [System.Net.HttpListener]::new()
$listener.Prefixes.Add("http://localhost:$Port/")
$listener.Start()
Write-Host "Serving $root (+ /plans/ -> $plans) at http://localhost:$Port/  (Ctrl+C to stop)"
$mime = @{
  ".html" = "text/html"; ".js" = "text/javascript"; ".mjs" = "text/javascript"
  ".wasm" = "application/wasm"; ".css" = "text/css"; ".json" = "application/json"
  ".ts"   = "application/typescript"; ".map" = "application/json"; ".pdf" = "application/pdf"
}
while ($listener.IsListening) {
  $ctx = $listener.GetContext()
  try {
    $path = [Uri]::UnescapeDataString($ctx.Request.Url.AbsolutePath).TrimStart('/')
    if ($path -eq '') { $path = 'index.html' }
    if ($path.StartsWith('plans/')) {
      $base = $plans
      $file = Join-Path $plans $path.Substring(6)
    } else {
      $base = $root
      $file = Join-Path $root $path
    }
    if ((Test-Path $file -PathType Leaf) -and ([IO.Path]::GetFullPath($file)).StartsWith($base)) {
      $bytes = [IO.File]::ReadAllBytes($file)
      $ext = [IO.Path]::GetExtension($file).ToLower()
      if ($mime.ContainsKey($ext)) { $ctx.Response.ContentType = $mime[$ext] }
      $ctx.Response.OutputStream.Write($bytes, 0, $bytes.Length)
    } else {
      $ctx.Response.StatusCode = 404
    }
  } catch { $ctx.Response.StatusCode = 500 }
  $ctx.Response.Close()
}
