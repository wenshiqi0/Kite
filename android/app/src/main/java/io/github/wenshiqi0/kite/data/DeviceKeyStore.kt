package io.github.wenshiqi0.kite.data

import android.content.Context
import android.os.Build
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import io.github.wenshiqi0.kite.core.SshKeyPair
import io.github.wenshiqi0.kite.core.describeSshKey
import io.github.wenshiqi0.kite.core.generateSshKey
import java.io.File
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * This device's SSH key. The private key never leaves the device: it is stored
 * encrypted with an AES key that lives in the Android Keystore.
 */
class DeviceKeyStore(context: Context) {
    private val file = File(context.noBackupFilesDir, "device_ed25519.enc")

    fun loadOrCreate(): SshKeyPair {
        if (file.exists()) {
            return describeSshKey(decrypt(file.readBytes()).toString(Charsets.UTF_8))
        }
        val pair = generateSshKey("kite@${Build.MODEL}")
        val pending = File(file.parentFile, "${file.name}.tmp")
        pending.writeBytes(encrypt(pair.privateKeyOpenssh.toByteArray(Charsets.UTF_8)))
        check(pending.renameTo(file)) { "could not store the device key" }
        return pair
    }

    private fun encrypt(plain: ByteArray): ByteArray {
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.ENCRYPT_MODE, wrappingKey())
        return cipher.iv + cipher.doFinal(plain)
    }

    private fun decrypt(stored: ByteArray): ByteArray {
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.DECRYPT_MODE, wrappingKey(), GCMParameterSpec(TAG_BITS, stored, 0, IV_BYTES))
        return cipher.doFinal(stored, IV_BYTES, stored.size - IV_BYTES)
    }

    private fun wrappingKey(): SecretKey {
        val keyStore = KeyStore.getInstance(KEYSTORE).apply { load(null) }
        (keyStore.getKey(ALIAS, null) as? SecretKey)?.let { return it }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, KEYSTORE)
        generator.init(
            KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .build(),
        )
        return generator.generateKey()
    }

    private companion object {
        const val KEYSTORE = "AndroidKeyStore"
        const val ALIAS = "kite-device-key-wrap"
        const val TRANSFORMATION = "AES/GCM/NoPadding"
        const val IV_BYTES = 12
        const val TAG_BITS = 128
    }
}
