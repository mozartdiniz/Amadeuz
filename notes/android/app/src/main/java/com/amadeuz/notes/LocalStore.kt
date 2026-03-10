package com.amadeuz.notes

import android.content.Context
import com.google.gson.Gson
import java.io.File

private data class LocalData(val folders: List<Folder>, val notes: List<Note>)

class LocalStore(context: Context) {
    private val file = File(context.filesDir, "data.json")
    private val gson = Gson()

    fun load(): Pair<List<Folder>, List<Note>> {
        if (!file.exists()) return Pair(emptyList(), emptyList())
        return try {
            val data = gson.fromJson(file.readText(), LocalData::class.java)
            Pair(data?.folders ?: emptyList(), data?.notes ?: emptyList())
        } catch (_: Exception) {
            Pair(emptyList(), emptyList())
        }
    }

    fun save(folders: List<Folder>, notes: List<Note>) {
        try {
            file.writeText(gson.toJson(LocalData(folders, notes)))
        } catch (_: Exception) {}
    }
}
