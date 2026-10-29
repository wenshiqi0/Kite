package io.github.wenshiqi0.kite.data

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import io.github.wenshiqi0.kite.core.SecretStore
import io.github.wenshiqi0.kite.core.SecretStoreException
import java.io.File
import java.io.FileOutputStream
import java.security.KeyStore
import java.security.MessageDigest
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/** Platform-only secret storage. The legacy file and wrapping key retain device identity. */
class DeviceKeyStore(context: Context) : SecretStore {
    private val directory = context.noBackupFilesDir

    @Synchronized
    override fun read(name: String): ByteArray? = safely {
        val file = fileFor(name)
        if (file.exists()) decrypt(file.readBytes()) else null
    }

    @Synchronized
    override fun write(name: String, value: ByteArray) {
        safely {
            val file = fileFor(name)
            val pending = File(directory, "${file.name}.tmp")
            val encrypted = encrypt(value)
            FileOutputStream(pending).use { it.write(encrypted); it.fd.sync() }
            check(pending.renameTo(file)) { "atomic secret write failed" }
        }
    }

    private inline fun <T> safely(action: () -> T): T = try {
        action()
    } catch (e: Exception) {
        throw SecretStoreException.Failure(e.javaClass.simpleName)
    }

    private fun fileFor(name: String): File {
        if (name == "ssh-ed25519") return File(directory, "device_ed25519.enc")
        val digest = MessageDigest.getInstance("SHA-256").digest(name.toByteArray(Charsets.UTF_8))
            .joinToString("") { "%02x".format(it) }
        return File(directory, "secret_$digest.enc")
    }

    private fun encrypt(plain: ByteArray): ByteArray {
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.ENCRYPT_MODE, wrappingKey())
        return cipher.iv + cipher.doFinal(plain)
    }

    private fun decrypt(stored: ByteArray): ByteArray {
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.DECRYPT_MODE, wrappingKey(), GCMParameterSpec(128, stored, 0, 12))
        return cipher.doFinal(stored, 12, stored.size - 12)
    }

    private fun wrappingKey(): SecretKey {
        val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (store.getKey(ALIAS, null) as? SecretKey)?.let { return it }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        generator.init(KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
            .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
            .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            .setKeySize(256).build())
        return generator.generateKey()
    }

    private companion object {
        const val ALIAS = "kite-device-key-wrap"
        const val TRANSFORMATION = "AES/GCM/NoPadding"
    }
}
