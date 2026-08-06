param(
    [Parameter(Mandatory = $true, Position = 0)]
    [string[]]$Path
)

$ErrorActionPreference = "Stop"

$Rsa = [System.Security.Cryptography.RSA]::Create(3072)
$Name = New-Object System.Security.Cryptography.X509Certificates.X500DistinguishedName(
    "CN=Vkit Local Build"
)
$Request = New-Object System.Security.Cryptography.X509Certificates.CertificateRequest(
    $Name,
    $Rsa,
    [System.Security.Cryptography.HashAlgorithmName]::SHA256,
    [System.Security.Cryptography.RSASignaturePadding]::Pkcs1
)
$Request.CertificateExtensions.Add(
    (New-Object System.Security.Cryptography.X509Certificates.X509BasicConstraintsExtension(
        $false, $false, 0, $true
    ))
)
$Request.CertificateExtensions.Add(
    (New-Object System.Security.Cryptography.X509Certificates.X509KeyUsageExtension(
        [System.Security.Cryptography.X509Certificates.X509KeyUsageFlags]::DigitalSignature,
        $true
    ))
)
$Usages = New-Object System.Security.Cryptography.OidCollection
[void]$Usages.Add(
    (New-Object System.Security.Cryptography.Oid(
        "1.3.6.1.5.5.7.3.3",
        "Code Signing"
    ))
)
$Request.CertificateExtensions.Add(
    (New-Object System.Security.Cryptography.X509Certificates.X509EnhancedKeyUsageExtension(
        $Usages,
        $true
    ))
)

$Certificate = $Request.CreateSelfSigned(
    (Get-Date).AddMinutes(-5),
    (Get-Date).AddYears(5)
)

try {
    foreach ($Item in $Path) {
        $Resolved = (Resolve-Path -LiteralPath $Item).Path
        $Result = Set-AuthenticodeSignature `
            -LiteralPath $Resolved `
            -Certificate $Certificate `
            -HashAlgorithm SHA256
        [pscustomobject]@{
            Path = $Resolved
            Status = $Result.Status
            StatusMessage = $Result.StatusMessage
            Subject = $Certificate.Subject
            Thumbprint = $Certificate.Thumbprint
            InstalledInCurrentUserMy = @(
                Get-ChildItem Cert:\CurrentUser\My |
                    Where-Object Thumbprint -eq $Certificate.Thumbprint
            ).Count
        }
    }
}
finally {
    $Certificate.Dispose()
    $Rsa.Dispose()
}
