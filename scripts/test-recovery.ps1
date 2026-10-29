param([switch]$SkipBaseline, [string]$LogPath = (Join-Path $env:APPDATA 'TwitchLite\twitchlite.log'), [switch]$FunctionsOnly)
$ErrorActionPreference = 'Stop'

function Invoke-Mpv([object[]]$Command) {
    $pipeName = 'twitchlite-qa'
    $active = Select-String -LiteralPath $LogPath -Pattern 'Player IPC: ' | Select-Object -Last 1
    if ($active) { $pipeName = ($active.Line -split 'Player IPC: ', 2)[1] -replace '^\\\\\.\\pipe\\', '' }
    $pipe = [System.IO.Pipes.NamedPipeClientStream]::new('.', $pipeName, [System.IO.Pipes.PipeDirection]::InOut)
    try {
        $pipe.Connect(3000)
        $writer = [System.IO.StreamWriter]::new($pipe)
        $writer.AutoFlush = $true
        $reader = [System.IO.StreamReader]::new($pipe)
        $writer.WriteLine((@{ command = $Command; request_id = 17 } | ConvertTo-Json -Compress))
        do {
            $read = $reader.ReadLineAsync()
            if (!$read.Wait(3000)) { throw 'mpv no respondió en 3 s' }
            $response = $read.Result | ConvertFrom-Json
        } while ($response -and $response.request_id -ne 17)
        if ($response.error -ne 'success') { throw "mpv: $($response.error)" }
        return $response.data
    } finally { $pipe.Dispose() }
}

function Refresh-Count {
    return @(Select-String -LiteralPath $LogPath -Pattern 'Refresh #').Count
}

function Get-MpvPosition {
    for ($attempt = 0; $attempt -lt 10; $attempt++) {
        try { return (Invoke-Mpv @('get_property', 'time-pos')) }
        catch {
            if ($_.Exception.Message -notlike '*property unavailable*') { throw }
            Start-Sleep -Seconds 1
        }
    }
    throw 'No apareció la posición después de reconectar'
}

if ($FunctionsOnly) { return }

if (!$SkipBaseline) {
    $baselineCount = Refresh-Count
    $before = Invoke-Mpv @('get_property', 'time-pos')
    Start-Sleep -Seconds 25
    $after = Invoke-Mpv @('get_property', 'time-pos')
    if ($after -le $before + 15) { throw 'La reproducción normal no avanzó lo esperado' }
    if ((Refresh-Count) -ne $baselineCount) { throw 'Hubo un refresh inesperado durante reproducción normal' }
    Write-Output "PASS: reproducción normal durante 25 s sin refresh innecesario (posición $before -> $after)."
}

$faultCount = Refresh-Count
$faultLine = @(Get-Content -LiteralPath $LogPath).Count
$started = [DateTime]::UtcNow
Invoke-Mpv @('set_property', 'pause', $true) | Out-Null
Write-Output 'Fallo controlado: mpv en pausa sin cambiar el estado de pausa de la aplicación.'
$recovered = $false
for ($i = 0; $i -lt 25; $i++) {
    Start-Sleep -Seconds 1
    if (!(Invoke-Mpv @('get_property', 'pause'))) {
        $recovered = $true
        break
    }
}
if (!$recovered) { throw 'La app no recuperó el reproductor en 25 s' }
$seconds = [Math]::Round(([DateTime]::UtcNow - $started).TotalSeconds, 1)
$resumed = Get-MpvPosition
Start-Sleep -Seconds 4
$advanced = Invoke-Mpv @('get_property', 'time-pos')
if ($advanced -le $resumed + 1) { throw 'mpv salió de pausa pero el directo no avanzó' }
$recoveryLines = @(Get-Content -LiteralPath $LogPath | Select-Object -Skip $faultLine)
if ($recoveryLines -match 'refresh manual|Ad skip attempt .*manual') { throw 'Una acción manual contaminó la prueba' }
if (!(($recoveryLines -match 'Ad skip attempt .*automatic') -or ($recoveryLines -match 'reproducción detenida / reintento'))) {
    throw 'No se registró recuperación automática para el fallo'
}
if ((Refresh-Count) -gt $faultCount + 1) { throw 'La recuperación entró en un bucle de recargas' }
Write-Output "PASS: recuperación automática en $seconds s y video avanzando ($resumed -> $advanced)."
